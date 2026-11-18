// i18n:extract: what it converts, what it refuses (writing nothing for those), that a second run changes nothing, that the
// output compiles and passes i18n:check once pt-BR exists, the Rust worklist, and that the codemod, the plan and the
// ratchet agree on the real tree (the detector and the codemod must see the same strings).
import { afterAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { compile } from "svelte/compiler";
import { type Report, extract, resolveTargets } from "./extract.ts";
import { areaOf } from "./extract/areas.ts";
import { MAX_FILES, buildPlan } from "./extract/plan.ts";
import { rustWorklist } from "./extract/rust.ts";
import { analyzeSvelte, importLine } from "./extract/svelte.ts";
import { CommandError } from "./edit.ts";
import { fix } from "./fix.ts";
import { scanRust, scanRustSource } from "./hardcoded/rust.ts";
import { SVELTE_ROOTS, scanSvelte, scanSvelteSource } from "./hardcoded/svelte.ts";
import { EN, META, PT, check, cleanup, editJson, errors, makeRepo, put, read } from "./testkit.ts";
import { loadCtx } from "./types.ts";
import { readText, walk } from "./util.ts";

afterAll(cleanup);

const ROOT = join(import.meta.dir, "..", "..");
const PANEL = "apps/desktop/src/lib/Panel.svelte";

/** One component with every case: text, attributes, a kit label prop, a common match, a variable, and what must stay. */
const COMPONENT = `<script lang="ts">
	import { Button, Select } from "@codeatlasdev/ui";
	let { item, n, user }: { item: { name: string }; n: number; user: string } = $props();
	const greet = (u: string) => u;
</script>

<h2>Display preferences</h2>
<input placeholder="Search the list" aria-label="Filter rows" />
<Select label="Display name" options={[]} />
<Button>Cancel</Button>
<Button>Save changes</Button>
<p>Delete {item.name} for good?</p>
<p>Saved {n} file{n === 1 ? "" : "s"}</p>
<p>Hello {greet(user)}</p>
<p>Press <kbd>x</kbd> to quit</p>
<p>Use <b>bold</b> sparingly</p>
<span>{n > 0 ? "Has files" : "No files"}</span>
<p>Copy the link https://example.com/docs</p>
<p>cancel</p>
<p>Shortcut ⌘K opens it</p>
<button title="Close panel" onclick={() => {}}>×</button>
<!-- i18n-ignore -->
<p>Debug build only</p>
`;

const TS_MODULE = `import { toast } from "@codeatlasdev/ui";
export function fail() {
	toast.error("Could not save the file");
	return { label: "Open recent project" };
}
`;

const reasons = (r: Report) => r.files.flatMap((f) => f.refused.map((x) => `${x.reason}:${x.text}`));

async function fixture(over: Record<string, string | null> = {}) {
	return makeRepo({ [PANEL]: COMPONENT, "apps/desktop/src/lib/state.ts": TS_MODULE, ...over });
}

const snapshot = (root: string, files: string[]) => files.map((f) => read(root, f)).join("\u0000");

describe("svelte codemod", () => {
	test("moves text, attributes, kit label props and variables into the area and calls m.key()", async () => {
		const root = await fixture();
		const r = extract(loadCtx(root), { targets: [PANEL], area: "app" });
		const out = read(root, PANEL);
		const en = JSON.parse(read(root, EN)) as Record<string, string>;
		const meta = JSON.parse(read(root, META)) as Record<string, string>;
		const key = (text: string) => Object.entries(en).find(([, v]) => v === text)?.[0] as string;

		expect(key("Display preferences")).toMatch(/^app_panel_display_preferences/);
		expect(out).toContain(`<h2>{m.${key("Display preferences")}()}</h2>`);
		expect(out).toContain(`placeholder={m.${key("Search the list")}()}`);
		expect(out).toContain(`aria-label={m.${key("Filter rows")}()}`);
		expect(out).toContain(`<Select label={m.${key("Display name")}()} options={[]} />`);
		expect(out).toContain(`title={m.${key("Close panel")}()}`);
		expect(key("Delete {name} for good?")).toBeDefined();
		expect(out).toContain(`{m.${key("Delete {name} for good?")}({ name: item.name })}`);
		expect(out).toContain('import { m } from "#lib/i18n/m.ts";');
		expect(out.indexOf('import { m }')).toBeGreaterThan(out.indexOf('import { Button, Select }'));
		expect(r.summary.converted).toBe(9);
		expect(r.written).toContain(PANEL);

		// each new key has a description that says what it is and where, and is not the text
		for (const k of Object.keys(en)) if (k.startsWith("app_panel_")) expect(meta[k]).toMatch(/Panel\.svelte/);
		expect(meta[key("Display preferences")]).toContain("Heading");
		expect(meta[key("Filter rows")]).toContain("aria-label");
		expect(meta[key("Delete {name} for good?")]).toContain("{name} is item.name");
		// pt-BR is never invented
		expect(Object.keys(JSON.parse(read(root, PT))).some((k) => k.startsWith("app_panel_"))).toBe(false);
	});

	test("reuses a common message when the English is the same, and never adds a duplicate", async () => {
		const root = await fixture();
		const r = extract(loadCtx(root), { targets: [PANEL], area: "app" });
		expect(read(root, PANEL)).toContain("<Button>{m.common_action_cancel()}</Button>");
		expect(Object.values(JSON.parse(read(root, EN)))).not.toContain("Cancel");
		expect(r.files[0].converted.find((c) => c.english === "Cancel")?.via).toBe("common");
		expect(r.summary.reused.common).toBe(1);
	});

	test("refuses, with file:line and a reason, what cannot be moved safely, and leaves it byte for byte", async () => {
		const root = await fixture();
		const r = extract(loadCtx(root), { targets: [PANEL], area: "app" });
		const out = read(root, PANEL);
		const line = (text: string) => COMPONENT.split("\n").findIndex((l) => l.includes(text)) + 1;
		const find = (reason: string) => r.files[0].refused.find((x) => x.reason === reason);

		expect(find("plural")).toMatchObject({ line: line("Saved {n}"), covers: 2 });
		expect(find("concatenation")).toMatchObject({ line: line("Hello {greet") });
		expect(find("inline-markup")).toBeDefined();
		expect(r.files[0].refused.filter((x) => x.reason === "inline-markup")).toHaveLength(5);
		expect(find("expression-literal")).toMatchObject({ line: line("Has files") });
		expect(find("shortcut")).toMatchObject({ line: line("Shortcut ⌘K") });
		expect(find("near-common")).toMatchObject({ line: line("<p>cancel") });
		for (const keep of ["Saved {n} file{n === 1", "Hello {greet(user)}", "Press <kbd>x</kbd> to quit", 'n > 0 ? "Has files"', "<p>cancel</p>", "Shortcut ⌘K opens it", "Debug build only", "https://example.com/docs"]) {
			expect(out).toContain(keep);
		}
		// the ratchet's remaining count is exactly what the codemod left
		expect(scanSvelteSource(out).length).toBe(scanSvelteSource(COMPONENT).length - r.summary.converted);
	});

	test("strings in .ts modules are listed, never edited", async () => {
		const root = await fixture();
		const r = extract(loadCtx(root), { targets: ["apps/desktop/src/lib/state.ts"], area: "app" });
		expect(read(root, "apps/desktop/src/lib/state.ts")).toBe(TS_MODULE);
		expect(r.files[0].refused.map((x) => [x.reason, x.line, x.text])).toEqual([
			["ts-module", 3, "Could not save the file"],
			["ts-module", 4, "Open recent project"],
		]);
	});

	test("a second run changes nothing", async () => {
		const root = await fixture();
		extract(loadCtx(root), { targets: [PANEL], area: "app" });
		const files = [PANEL, EN, META, PT, "i18n/surfaces.json"];
		const before = snapshot(root, files);
		const again = extract(loadCtx(root), { targets: [PANEL], area: "app" });
		expect(again.summary.converted).toBe(0);
		expect(again.summary.keys).toBe(0);
		expect(snapshot(root, files)).toBe(before);
	});

	test("--dry-run writes nothing and reports the same numbers", async () => {
		const root = await fixture();
		const files = [PANEL, EN, META, "i18n/surfaces.json"];
		const before = snapshot(root, files);
		const dry = extract(loadCtx(root), { targets: [PANEL], area: "app", dryRun: true });
		expect(snapshot(root, files)).toBe(before);
		expect(dry.written).toEqual([]);
		const real = extract(loadCtx(root), { targets: [PANEL], area: "app" });
		expect(real.summary).toEqual(dry.summary);
	});

	test("the output compiles, and i18n:check fails only for the missing pt-BR until it exists", async () => {
		const root = await fixture();
		extract(loadCtx(root), { targets: [PANEL], area: "app" });
		expect(() => compile(read(root, PANEL), { filename: PANEL, generate: "client" })).not.toThrow();

		const open = await check(root);
		expect(new Set(errors(open).map((f) => f.rule))).toEqual(new Set(["missing"]));

		// what `bun run i18n:translate` (or the migrating agent) does: every new key gets a pt-BR value
		const en = JSON.parse(read(root, EN)) as Record<string, string>;
		const pt = JSON.parse(read(root, PT)) as Record<string, string>;
		editJson(root, PT, (o) => {
			for (const [k, v] of Object.entries(en)) if (!(k in pt)) o[k] = `Texto ${v}`;
		});
		fix(loadCtx(root));
		expect(errors(await check(root))).toEqual([]);
	});

	test("a new area is registered under the surface of the file; the budget is enforced before anything is written", async () => {
		const root = await fixture();
		extract(loadCtx(root), { targets: [PANEL] });
		expect(read(root, "i18n/surfaces.json")).toContain('"workbench"');
		expect(errors(await check(root)).every((f) => f.rule === "missing")).toBe(true);

		const tight = await fixture();
		put(tight, "i18n/surfaces.json", read(tight, "i18n/surfaces.json").replace('"keys": 150', '"keys": 6'));
		const before = snapshot(tight, [PANEL, EN, META]);
		expect(() => extract(loadCtx(tight), { targets: [PANEL], area: "app" })).toThrow(/budget/);
		expect(snapshot(tight, [PANEL, EN, META])).toBe(before);
	});

	test("refuses to extract into common, or into an area the surface does not consume", async () => {
		const root = await fixture();
		expect(() => extract(loadCtx(root), { targets: [PANEL], area: "common" })).toThrow(CommandError);
		expect(() => extract(loadCtx(root), { targets: [PANEL], area: "cli" })).toThrow(/not consumed by desktop/);
	});

	test("a file that already uses an identifier m is refused, not broken", async () => {
		const src = `<script>\n\tlet { items } = $props();\n\tlet m = $derived(items[0]);\n</script>\n<i>{m}</i>\n<p>Shadow check</p>\n`;
		const a = analyzeSvelte(src);
		expect(a.candidates).toEqual([]);
		expect(a.refused.map((x) => x.reason)).toEqual(["shadowed-m"]);
		const inner = `<script>\n\tlet { items } = $props();\n</script>\n{#each items as m}<i>{m}</i>{/each}\n<p>Outside the loop</p>\n`;
		expect(analyzeSvelte(inner).candidates.map((c) => c.english)).toEqual(["Outside the loop"]);
	});

	test("a component with no script gets one, and the site imports Paraglide directly", async () => {
		const root = await makeRepo({ "apps/desktop/src/lib/Plain.svelte": "<p>Plain words</p>\n" });
		extract(loadCtx(root), { targets: ["apps/desktop/src/lib/Plain.svelte"], area: "app" });
		expect(read(root, "apps/desktop/src/lib/Plain.svelte")).toMatch(/^<script lang="ts">\n\timport \{ m \} from "#lib\/i18n\/m\.ts";\n<\/script>\n\n<p>\{m\.app_plain_plain_words\(\)\}<\/p>/);
		expect(importLine("site", "site/src/components/islands/Hi.svelte")).toBe('import { m } from "../../lib/paraglide/messages.js";');
	});
});

describe("rust worklist", () => {
	const RS = `use atlas_core::x;
fn run(n: usize, name: &str, p: &std::path::Path) -> anyhow::Result<()> {
    println!("Nothing to do");
    println!("Deleted {n} files");
    println!("Hello {}", name);
    println!("{:>8} files", n);
    println!("\\x1b[1mDone\\x1b[0m");
    println!("{{\\"ok\\": true}}");
    println!("{}", serde_json::to_string(&p)?);
    println!("step: installing");
    // i18n-ignore: debug
    println!("Debug only");
    eprintln!("Removed {} from {}", p.display(), name);
    anyhow::bail!("could not read {}", name);
}
`;

	test("classifies every site the ratchet counts, with a suggested key and call for the mechanical ones", () => {
		const items = rustWorklist("crates/atlas-cli/src/commands/x.rs", RS, "cmd_misc").filter((i) => i.kind !== "uncounted");
		expect(items.length).toBe(scanRustSource(RS).length);
		const kind = (fmt: string) => items.find((i) => i.format.startsWith(fmt));
		expect(kind("Nothing to do")).toMatchObject({ kind: "text", call: 'println!("{}", t!("cmd_misc_nothing_to_do"))' });
		expect(kind("Deleted {n} files")).toMatchObject({ kind: "vars", english: "Deleted {n} files", call: 'println!("{}", t!("cmd_misc_deleted_files", n = n))' });
		expect(kind("Hello {}")).toMatchObject({ kind: "vars", english: "Hello {name}" });
		expect(kind("{:>8} files")?.kind).toBe("judgement");
		expect(kind("{:>8} files")?.reasons[0]).toContain("format-spec");
		expect(kind("\\x1b")?.reasons[0]).toContain("ansi");
		expect(kind("{{")?.kind).toBe("machine");
		expect(kind("Removed {}")?.kind).toBe("vars"); // atlas-cli: a human line; atlas-host would be a log
		expect(kind("could not read")?.notes.join(" ")).toContain("a program never parses human text");
		expect(items.some((i) => i.format.includes("Debug only") || i.format.startsWith("step:"))).toBe(false);
	});

	test("a daemon's eprintln! is a log candidate, and a .rs target is never rewritten", async () => {
		const items = rustWorklist("crates/atlas-host/src/server.rs", RS, "hostd_daemon");
		expect(items.find((i) => i.format.startsWith("Removed {}"))?.kind).toBe("log");
		const root = await makeRepo({ "crates/atlas-cli/src/commands/x.rs": RS });
		const r = extract(loadCtx(root), { targets: ["crates/atlas-cli/src/commands/x.rs"] });
		expect(read(root, "crates/atlas-cli/src/commands/x.rs")).toBe(RS);
		expect(r.files[0].type).toBe("rust");
		expect(r.files[0].area).toBe("cmd_misc");
		expect(r.summary.rust?.machine).toBe(1);
	});
});

describe("targets", () => {
	test("a path, a directory and a glob resolve to repo-relative files; a missing path says so", async () => {
		const root = await fixture();
		expect(resolveTargets(root, [PANEL])).toEqual([PANEL]);
		expect(resolveTargets(root, ["apps/desktop/src/lib"])).toEqual([PANEL, "apps/desktop/src/lib/state.ts"]);
		expect(resolveTargets(root, ["apps/desktop/src/lib/*.svelte"])).toEqual([PANEL]);
		expect(resolveTargets(root, [join(root, PANEL)])).toEqual([PANEL]);
		expect(() => resolveTargets(root, ["nope/x.svelte"])).toThrow(/no such file/);
	});
});

describe("the real tree", () => {
	test("the codemod sees exactly the strings the hardcoded ratchet counts (converted + left = flagged), per Svelte file", () => {
		let flagged = 0;
		let seen = 0;
		for (const dir of ["apps/desktop/src", "site/src"]) {
			for (const p of walk(join(ROOT, dir), (f) => f.endsWith(".svelte"))) {
				const text = readText(p);
				let counted: number;
				try {
					counted = scanSvelteSource(text).length;
				} catch {
					continue;
				}
				const a = analyzeSvelte(text);
				const got = [...a.candidates, ...a.refused].reduce((n, x) => n + x.covers, 0);
				expect({ p, got }).toEqual({ p, got: counted });
				flagged += counted;
				seen += got;
			}
		}
		expect(seen).toBe(flagged);
	});

	test("the Rust worklist lists exactly the sites the ratchet counts, per file", () => {
		for (const file of scanRust(ROOT).keys()) {
			const text = readText(join(ROOT, file));
			const items = rustWorklist(file, text, areaOf(file) as string).filter((i) => i.kind !== "uncounted");
			expect({ file, n: items.length }).toEqual({ file, n: scanRustSource(text).length });
		}
	});

	test("every file with hardcoded strings belongs to an area, and each batch of the plan is one PR-sized unit that fits the catalog budget", () => {
		const plan = buildPlan(ROOT);
		const planned = plan.flatMap((b) => b.files.map((f) => f.file)).sort();
		const today = [...scanSvelte(ROOT, SVELTE_ROOTS.map((r) => r.dir)).found.keys(), ...scanRust(ROOT).keys()].sort();
		expect(planned).toEqual(today);
		expect(plan.flatMap((b) => b.areas)).not.toContain("unmapped");
		for (const b of plan) expect(b.files.length).toBeLessThanOrEqual(MAX_FILES);
		const perArea = new Map<string, number>();
		for (const b of plan) for (const f of b.files) perArea.set(areaOf(f.file) as string, (perArea.get(areaOf(f.file) as string) ?? 0) + f.strings);
		for (const [area, strings] of perArea) expect({ area, fits: strings <= 150 }).toEqual({ area, fits: true });
	});
});
