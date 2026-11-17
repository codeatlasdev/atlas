// The message algebra the generators and the rules share. A catalog value is a string (`Hello {name}`) or one variants
// object (`declarations`, `selectors`, `match`: plural, exact-zero, `os`). A string may reference another message,
// `{@common_os_trash}`, and a reference is INLINED: the result selects on the union of both messages' selectors, so
// "Moved to {@common_os_trash}" becomes an `os` select without anyone writing one.
//
// Variants are matched the MF2 way: a variant matches when every selector either equals its literal or the variant
// has `*` there; among the matching ones the earliest selector decides, a literal beating `*`.

export interface Variant {
	declarations?: string[];
	selectors?: string[];
	match: Record<string, string>;
}

export type MsgValue = string | Variant[];

/** One variant: the literal (or `*`) it needs for each selector, and its text. */
export interface Arm {
	cond: Record<string, string>;
	text: string;
}

export interface Msg {
	selectors: string[];
	declarations: string[];
	arms: Arm[];
}

export interface SelectorInfo {
	name: string;
	/** The input variable the selector matches on (`count`, `os`). */
	var: string;
	kind: "plural" | "select";
}

export class MessageError extends Error {
	constructor(
		readonly rule: "dangling-ref" | "unsupported",
		message: string,
	) {
		super(message);
	}
}

/** `countExact=0, countPlural=*` or `os=mac` or `*`: selector name -> literal (`*` = anything). */
export function parseMatchKey(key: string): Record<string, string> {
	const cond: Record<string, string> = {};
	for (const part of key.split(/[\s,]+/).filter(Boolean)) {
		const eq = part.indexOf("=");
		if (eq > 0) cond[part.slice(0, eq)] = part.slice(eq + 1);
	}
	return cond;
}

export const matchKey = (cond: Record<string, string>, selectors: string[]) => selectors.map((s) => `${s}=${cond[s] ?? "*"}`).join(", ");

export function normalize(v: MsgValue): Msg {
	if (typeof v === "string") return { selectors: [], declarations: [], arms: [{ cond: {}, text: v }] };
	if (v.length !== 1) throw new MessageError("unsupported", "a message has exactly one variants object");
	const [variant] = v;
	return {
		selectors: variant.selectors ?? [],
		declarations: variant.declarations ?? [],
		arms: Object.entries(variant.match).map(([k, text]) => ({ cond: parseMatchKey(k), text })),
	};
}

/** The value a message is written as: a plain string when nothing selects, else one variants object. */
export function toValue(m: Msg): MsgValue {
	if (m.selectors.length === 0) return m.arms[0]?.text ?? "";
	const match: Record<string, string> = {};
	for (const a of m.arms) match[matchKey(a.cond, m.selectors)] = a.text;
	return [{ declarations: m.declarations, selectors: m.selectors, match }];
}

/** What a selector matches on and how: `local n = count: plural` is a plural of `count`; anything else a plain select. */
export function selectorInfo(m: Msg, name: string): SelectorInfo {
	for (const d of m.declarations) {
		const local = /^local\s+\$?([^\s=:]+)\s*=\s*\{?\s*\$?([^\s=:}]+)\s*(?::\s*([^\s=:}]+)(.*?))?\}?$/.exec(d.trim());
		if (local?.[1] === name) {
			const ordinal = /type\s*=\s*\|?ordinal/.test(local[4] ?? "");
			return { name, var: local[2], kind: local[3] === "plural" && !ordinal ? "plural" : "select" };
		}
	}
	return { name, var: name, kind: "select" };
}

const REF = /\{@([a-z][a-z0-9_]*)\}/g;

export const refsIn = (text: string) => [...text.matchAll(REF)].map((m) => m[1]);

export const refsOf = (v: MsgValue): string[] => [...new Set(normalize(v).arms.flatMap((a) => refsIn(a.text)))];

const lexLess = (a: number[], b: number[]) => {
	for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] < b[i];
	return false;
};

/** Rank of an arm: 0 for a literal, 1 for `*`, per selector in order; lower ranks win lexicographically. */
const rankOf = (m: { selectors: string[] }, a: Arm) => m.selectors.map((s) => ((a.cond[s] ?? "*") === "*" ? 1 : 0));

/** The best of `arms` (first on a tie), or undefined when there are none. */
function best(m: { selectors: string[] }, arms: Arm[]): Arm | undefined {
	let win: Arm | undefined;
	let winRank: number[] = [];
	for (const a of arms) {
		const rank = rankOf(m, a);
		if (!win || lexLess(rank, winRank)) {
			win = a;
			winRank = rank;
		}
	}
	return win;
}

/** The arm that applies when selector `s` has value `assign[s]` (`*` = none of the literals): all selectors must
 * match, the earliest selector decides, a literal beats `*`. */
export const pick = (m: Msg, assign: Record<string, string>): Arm | undefined =>
	best(
		m,
		m.arms.filter((arm) => m.selectors.every((s) => (arm.cond[s] ?? "*") === "*" || arm.cond[s] === assign[s])),
	);

const MAX_ARMS = 4096;

function product(domains: string[][]): Record<string, string>[] {
	return domains.reduce<Record<string, string>[]>((acc, values, i) => acc.flatMap((a) => values.map((v) => ({ ...a, [String(i)]: v }))), [{}]);
}

/** Drop what changes nothing: a selector whose value never changes the text, an arm equal to its `*` sibling. */
export function simplify(m: Msg): Msg {
	let arms = m.arms;
	let selectors = m.selectors;
	for (let changed = true; changed; ) {
		changed = false;
		for (const s of selectors) {
			const groups = new Map<string, Arm[]>();
			for (const a of arms) {
				const rest = JSON.stringify(selectors.filter((x) => x !== s).map((x) => a.cond[x] ?? "*"));
				groups.set(rest, [...(groups.get(rest) ?? []), a]);
			}
			const uniform = [...groups.values()].every((g) => g.every((a) => a.text === g[0].text));
			if (uniform) {
				arms = [...groups.values()].map((g) => ({ cond: Object.fromEntries(Object.entries(g[0].cond).filter(([k]) => k !== s)), text: g[0].text }));
				selectors = selectors.filter((x) => x !== s);
				changed = true;
				break;
			}
			const redundant = arms.filter(
				(a) => (a.cond[s] ?? "*") !== "*" && arms.some((b) => (b.cond[s] ?? "*") === "*" && b.text === a.text && selectors.every((x) => x === s || (a.cond[x] ?? "*") === (b.cond[x] ?? "*"))),
			);
			if (redundant.length) {
				arms = arms.filter((a) => !redundant.includes(a));
				changed = true;
				break;
			}
		}
	}
	const keep = new Set(selectors);
	return { selectors, declarations: m.declarations.filter((d) => keepsDeclaration(d, keep)), arms };
}

/** Inputs stay (text may use them as {var}); a `local` declaration stays only while a selector uses it. */
const keepsDeclaration = (d: string, keep: Set<string>) => {
	const local = /^local\s+\$?([^\s=:]+)/.exec(d.trim());
	return !local || keep.has(local[1]);
};

/** `value` with every `{@key}` replaced by the referenced message, selectors composed. Unchanged when it has none.
 * `shrink: false` keeps selectors that never change the text (a translation whose term reads the same on every OS still
 * selects on `os`, which is what its shape must be compared by).
 * Throws a MessageError for a reference that does not exist, a cycle, or two messages that select on one name
 * differently. */
export function inlineRefs(value: MsgValue, lookup: (key: string) => MsgValue | undefined, stack: string[] = [], shrink = true): MsgValue {
	const m = normalize(value);
	const refs = [...new Set(m.arms.flatMap((a) => refsIn(a.text)))];
	if (refs.length === 0) return value;
	const resolved = new Map<string, Msg>();
	for (const r of refs) {
		if (stack.includes(r)) throw new MessageError("dangling-ref", `cycle: ${[...stack, r].join(" -> ")}`);
		const target = lookup(r);
		if (target === undefined) throw new MessageError("dangling-ref", `{@${r}} is not a message`);
		resolved.set(r, normalize(inlineRefs(target, lookup, [...stack, r], shrink)));
	}
	const sources = [m, ...resolved.values()];
	const selectors = [...new Set(sources.flatMap((s) => s.selectors))];
	const infos = new Map<string, SelectorInfo>();
	for (const s of sources) {
		for (const name of s.selectors) {
			const info = selectorInfo(s, name);
			const seen = infos.get(name);
			if (seen && (seen.var !== info.var || seen.kind !== info.kind)) {
				throw new MessageError("unsupported", `selector "${name}" is declared differently by the message and what it references`);
			}
			infos.set(name, info);
		}
	}
	const domains = selectors.map((s) => [...new Set([...sources.flatMap((x) => x.arms.map((a) => a.cond[s])).filter((v): v is string => v !== undefined && v !== "*"), "*"])]);
	if (domains.reduce((n, d) => n * d.length, 1) > MAX_ARMS) throw new MessageError("unsupported", "too many combinations of selectors");
	const arms: Arm[] = [];
	for (const raw of product(domains)) {
		const assign = Object.fromEntries(selectors.map((s, i) => [s, raw[String(i)]]));
		const own = pick(m, assign);
		if (!own) continue;
		let ok = true;
		const text = own.text.replace(REF, (_x, r: string) => {
			const arm = pick(resolved.get(r) as Msg, assign);
			if (!arm) ok = false;
			return arm?.text ?? "";
		});
		if (ok) arms.push({ cond: assign, text });
	}
	const declarations = [...new Set(sources.flatMap((s) => s.declarations))];
	const msg = { selectors, declarations, arms };
	return toValue(shrink ? simplify(msg) : msg);
}

export type Node = { text: string } | { sel: SelectorInfo; branches: { literal: string; node: Node }[]; fallback: Node };

const same = (a: Node, b: Node) => JSON.stringify(a) === JSON.stringify(b);

/** The message as nested selects: group by the first selector's literal, recurse on the rest, `*` is the fallback.
 * Throws when some value has no variant (no catch-all) or the message does not fit that shape. */
export function buildTree(m: Msg): Node {
	const infos = m.selectors.map((s) => selectorInfo(m, s));
	const rec = (arms: Arm[], i: number): Node | undefined => {
		if (i === m.selectors.length) {
			const win = best(m, arms);
			return win && { text: win.text };
		}
		const s = m.selectors[i];
		const literals = [...new Set(arms.map((a) => a.cond[s] ?? "*").filter((v) => v !== "*"))];
		const fallback = rec(
			arms.filter((a) => (a.cond[s] ?? "*") === "*"),
			i + 1,
		);
		const branches = literals
			.map((literal) => ({ literal, node: rec(arms.filter((a) => (a.cond[s] ?? "*") === "*" || a.cond[s] === literal), i + 1) }))
			.filter((b): b is { literal: string; node: Node } => b.node !== undefined)
			.filter((b) => !fallback || !same(b.node, fallback));
		if (!fallback) throw new MessageError("unsupported", `no catch-all variant for selector "${s}"`);
		return branches.length === 0 ? fallback : { sel: infos[i], branches, fallback };
	};
	const tree = rec(m.arms, 0);
	if (!tree) throw new MessageError("unsupported", "no variants");
	return mergeSameVar(tree);
}

/** `{ $n -> [0] .. *[other] { $n -> [one] .. *[other] .. } }` is one select on `$n`: exact numbers and categories
 * mix in one select (ICU, and the Rust evaluator). Only when every exact arm is a plain text. */
function mergeSameVar(n: Node): Node {
	if (!("sel" in n)) return n;
	const branches = n.branches.map((b) => ({ literal: b.literal, node: mergeSameVar(b.node) }));
	const fallback = mergeSameVar(n.fallback);
	if ("sel" in fallback && fallback.sel.var === n.sel.var && branches.every((b) => "text" in b.node)) {
		return { sel: fallback.sel, branches: [...branches, ...fallback.branches], fallback: fallback.fallback };
	}
	return { sel: n.sel, branches, fallback };
}
