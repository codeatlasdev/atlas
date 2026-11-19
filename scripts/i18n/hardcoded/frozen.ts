// Frozen messages: a message function called once and stored keeps the language of that moment, and the desktop switches
// language live (the locale is Svelte `$state`). Found on the AST, in the places a value is computed once:
//   - the top level of a `.ts` / `.svelte.ts` module and of a `<script module>` block, including every object, array,
//     template and `$state(...)` initial value built there (`const ITEMS = [{ label: m.a() }]`, `register({ label: m.a() })`);
//   - class fields, static blocks and constructors (a store instance lives for the whole session);
//   - the instance script of a `+layout.svelte` (the shell never remounts);
//   - a command registered with the keymap's `register([...])` (the palette reads `label` every time it lists, the label
//     was computed once, when the component mounted) and the body of a function that returns `Command[]`.
// Not frozen: function and method bodies, getters, `$derived` / `$effect`, templates, a message function kept uncalled
// (`label: m.a`), the instance script of an ordinary component (it re-runs per mount). A genuine case (machine text that
// stays English) carries `// i18n-allow frozen: <why>` on its line or the line above.
// A message reached through another function (`const ITEMS = build()`, `build` calls `m.a()`) is not followed.
import { parse } from "svelte/compiler";
import { messageBindings } from "../messageModule.ts";
import type { Found, Violation } from "../baseline.ts";
import { lineOf, readText, rel, walk } from "../util.ts";

/** Where messages are called from: the desktop app and the site (`--surface` selects one). */
export const FROZEN_ROOTS: { surface: string; dir: string }[] = [
	{ surface: "desktop", dir: "apps/desktop/src" },
	{ surface: "site", dir: "site/src" },
];

const WAIVER = /i18n-allow frozen:\s*\S/;
const TEST_FILE = /\.(?:test|spec)\.ts$|(?:^|\/)(?:test|__fixtures__|paraglide|\.i18n)\/|\.d\.ts$|(?:^|\/)messages\.gen\.ts$/;
const LAYOUT = /(?:^|\/)\+layout(?:@[^./]*)?\.svelte$/;
const FUNCTIONS = new Set(["FunctionDeclaration", "FunctionExpression", "ArrowFunctionExpression"]);
const KEYMAP_MODULE = /(?:^|\/)keymap\.svelte(?:\.ts)?$/;
const REACTIVE = /^\$(?:derived|effect|inspect)\b/;

type Node = { type: string; start: number; [k: string]: unknown };
const isNode = (v: unknown): v is Node => typeof v === "object" && v !== null && typeof (v as Node).type === "string" && typeof (v as Node).start === "number";

/** `$derived`, `$derived.by`, `$effect.pre`: the argument runs reactively, never once. */
function isReactiveRune(callee: Node): boolean {
	if (callee.type === "Identifier") return REACTIVE.test(callee.name as string);
	return callee.type === "MemberExpression" && isNode(callee.object) && callee.object.type === "Identifier" && REACTIVE.test(callee.object.name as string);
}

export function scanFrozenSource(file: string, source: string): Violation[] {
	if (TEST_FILE.test(file) || !(file.endsWith(".ts") || file.endsWith(".svelte"))) return [];
	const bindings = messageBindings(source);
	if (bindings.spaces.length === 0 && bindings.named.length === 0) return [];
	const spaces = new Set(bindings.spaces);
	const locals = new Set(bindings.named.map((n) => n.local));

	const isSvelte = file.endsWith(".svelte");
	// A module is parsed as the instance script of a component (no newline added, so lines stay the file's).
	const text = isSvelte ? source : `<script lang="ts">${source}</script>`;
	const ast = parse(text, { modern: true }) as unknown as { module?: { content: Node }; instance?: { content: Node } };
	/** Each script and whether its top level runs once for the whole session (a module, a layout) or per mount. */
	const scripts: { program: Node; once: boolean }[] = [];
	if (ast.module) scripts.push({ program: ast.module.content, once: true });
	if (ast.instance) scripts.push({ program: ast.instance.content, once: !isSvelte || LAYOUT.test(file) });

	// The keymap's `register` and `Command` under whatever names this file imports them.
	const registers = new Set<string>();
	const commandTypes = new Set<string>();
	for (const [, clause, spec] of source.matchAll(/import\s+(?:type\s+)?([^;'"]*?)\s*from\s*["']([^"']+)["']/g)) {
		if (!KEYMAP_MODULE.test(spec)) continue;
		for (const part of /\{([^}]*)\}/.exec(clause)?.[1].split(",") ?? []) {
			const [imported, local = imported] = part.trim().replace(/^type\s+/, "").split(/\s+as\s+/);
			if (imported === "register") registers.add(local);
			if (imported === "Command") commandTypes.add(local);
		}
	}
	const returnsCommands = (fn: Node): boolean => {
		const rt = fn.returnType;
		if (!isNode(rt)) return false;
		const t = text.slice(rt.start, (rt as unknown as { end: number }).end);
		return [...commandTypes].some((c) => new RegExp(`\\b${c}\\s*\\[\\]|Array<\\s*${c}\\s*>`).test(t));
	};

	// `const { key: local } = m` makes `local()` a message call too.
	for (const { program } of scripts) {
		for (const stmt of (program.body as Node[]) ?? []) {
			if (stmt.type !== "VariableDeclaration") continue;
			for (const d of stmt.declarations as Node[]) {
				const id = d.id as Node;
				const init = d.init as Node | null;
				if (id.type !== "ObjectPattern" || !init || init.type !== "Identifier" || !spaces.has(init.name as string)) continue;
				for (const p of id.properties as Node[]) {
					const v = p.value as Node | undefined;
					if (p.type === "Property" && v?.type === "Identifier") locals.add(v.name as string);
				}
			}
		}
	}

	const isMessageCall = (callee: Node): boolean => {
		let n = callee;
		while (n.type === "MemberExpression" && isNode(n.object)) n = n.object;
		return n.type === "Identifier" && (n === callee ? locals : spaces).has(n.name as string);
	};

	const out: Violation[] = [];
	const lines = source.split("\n");
	const flag = (n: Node) => {
		const line = lineOf(text, n.start);
		out.push({ line, text: (lines[line - 1] ?? "").trim().slice(0, 80) });
	};

	/** `once`: this code runs a single time (module scope, a class field, a registered command list), so a message
	 * called here is frozen. Outside it the walk only looks for places that start such a context. */
	const visit = (v: unknown, once: boolean): void => {
		if (Array.isArray(v)) return void v.forEach((x) => visit(x, once));
		if (!isNode(v)) return;
		if (FUNCTIONS.has(v.type)) {
			// A function body runs when called (the walk still looks inside it for a `register`); one that returns the keymap's `Command[]` runs once, to register.
			visit(v.body, returnsCommands(v));
			return;
		}
		if (v.type === "CallExpression") {
			const callee = v.callee as Node;
			if (isReactiveRune(callee)) return;
			if (FUNCTIONS.has(callee.type)) visit(callee.body, once); // an immediately invoked function runs now
			else {
				if (once && isMessageCall(callee)) flag(v);
				visit(callee, once);
			}
			visit(v.arguments, callee.type === "Identifier" && registers.has(callee.name as string) ? true : once);
			return;
		}
		for (const [k, child] of Object.entries(v)) if (k !== "loc" && k !== "type") visit(child, once);
	};
	for (const { program, once } of scripts) visit(program.body, once);

	// The waiver sits on the offending line or the one above, and says why.
	return out.filter((o) => !WAIVER.test(lines[o.line - 1] ?? "") && !WAIVER.test(lines[o.line - 2] ?? ""));
}

/** Every desktop and site module under `dirs` (repo-relative), files with no frozen call omitted; unreadable files come back in `unparsable`. */
export function scanFrozen(root: string, dirs: string[]): { found: Found; unparsable: { file: string; message: string }[] } {
	const found: Found = new Map();
	const unparsable: { file: string; message: string }[] = [];
	for (const dir of dirs) {
		for (const path of walk(`${root}/${dir}`, (p) => p.endsWith(".ts") || p.endsWith(".svelte"))) {
			const file = rel(root, path);
			try {
				const v = scanFrozenSource(file, readText(path));
				if (v.length) found.set(file, v);
			} catch (e) {
				unparsable.push({ file, message: (e as Error).message });
			}
		}
	}
	return { found, unparsable };
}
