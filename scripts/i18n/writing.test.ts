// The writing lint: rule mechanics on a small rules file, then the shipped flagged-terms.yaml on real text.
import { afterAll, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { readBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { loadCtx } from "./types.ts";
import { EN, META, PT, addKey, check, cleanup, editJson, errors, finding, makeRepo, put, read, rulesOf } from "./testkit.ts";
import { allowsIn, lintMarkdown, loadRules, markdownProse } from "./writing.ts";

afterAll(cleanup);

const RULES_FILE = "i18n/style/flagged-terms.yaml";
const MINI = `version: 1
rules:
  - id: em-dash
    lang: any
    severity: error
    word: false
    pattern: '[\\u2014\\u2015]'
    why: House rule.
    suggest: Pick the real relation.
  - id: en-hype
    lang: en
    severity: error
    pattern: 'seamless(?:ly)?|unlock(?:s|ed|ing)?'
    except: ['unlock(?:s|ed|ing)? (?:the )?keychain']
    why: Hype.
    suggest: Say what happens.
  - id: pt-calque
    lang: pt-BR
    severity: error
    pattern: 'realizar'
    why: Calque.
    suggest: fazer
  - id: ui-bang
    lang: any
    scope: ui
    severity: warn
    word: false
    pattern: '!\\s*$'
    flags: 'u'
    why: Alarm.
    suggest: Use a period.
  - id: docs-only
    lang: any
    scope: docs
    severity: error
    pattern: 'simply'
    why: Filler.
    suggest: Cut it.
  - id: case-sensitive
    lang: en
    severity: warn
    pattern: 'ALLCAPS'
    flags: 'u'
    why: Shouting.
    suggest: Lowercase.
`;

const ctxOf = (root: string) => loadCtx(root);
const writing = async (root: string) => (await check(root, "writing")).filter((f) => f.severity !== "info");

async function repo(rules = MINI) {
	const root = await makeRepo({ [RULES_FILE]: rules });
	return root;
}

describe("rules file", () => {
	test("the shipped flagged-terms.yaml compiles with no problem and has rules", () => {
		const real = loadRules(join(import.meta.dir, "..", ".."));
		expect(real.problems).toEqual([]);
		expect(real.rules.length).toBeGreaterThan(20);
	});

	test("a bad pattern, a duplicate id, an unknown severity or a missing field is an error on the YAML itself", async () => {
		const bad = `rules:
  - { id: a, lang: any, severity: error, pattern: '(', why: x, suggest: y }
  - { id: b, lang: any, severity: error, pattern: 'x', why: x, suggest: y }
  - { id: b, lang: any, severity: error, pattern: 'x', why: x, suggest: y }
  - { id: c, lang: any, severity: fatal, pattern: 'x', why: x, suggest: y }
  - { id: d, lang: fr, severity: error, pattern: 'x', why: x, suggest: y }
  - { lang: any, severity: error, pattern: 'x' }
`;
		const root = await repo(bad);
		const fs = (await check(root, "writing")).filter((f) => f.rule === "writing-rules");
		expect(fs.map((f) => f.key)).toEqual(expect.arrayContaining(["a", "b", "c", "d"]));
		expect(fs.length).toBe(5);
		expect(fs.every((f) => f.severity === "error" && f.file === RULES_FILE)).toBe(true);
	});

	test("no rules file means no writing findings", async () => {
		const root = await makeRepo();
		expect(await writing(root)).toEqual([]);
	});
});

describe("catalog values (scope ui)", () => {
	test("a valid repo passes the mini rules", async () => {
		expect(await writing(await repo())).toEqual([]);
	});

	test("an em dash in an English or pt-BR value is an error at the key and line", async () => {
		const root = await repo();
		editJson(root, EN, (o) => void (o.app_save_button = "Save \u2014 now"));
		editJson(root, PT, (o) => void (o.app_save_button = "Salvar \u2014 agora"));
		const fs = (await writing(root)).filter((f) => f.rule === "writing-em-dash");
		expect(fs.map((f) => f.file).sort()).toEqual([EN, PT]);
		expect(fs.every((f) => f.severity === "error" && f.key === "app_save_button" && (f.line ?? 0) > 0)).toBe(true);
	});

	test("lang: an English rule does not fire on pt-BR text, a pt-BR rule not on English, any on both", async () => {
		const root = await repo();
		editJson(root, PT, (o) => void (o.app_save_button = "Seamless"));
		expect(rulesOf(await writing(root))).not.toContain("writing-en-hype");
		editJson(root, EN, (o) => void (o.app_save_button = "Seamless save"));
		expect(rulesOf(await writing(root))).toEqual(["writing-en-hype"]);
		editJson(root, EN, (o) => void (o.app_save_button = "Realizar"));
		expect(rulesOf(await writing(root))).not.toContain("writing-pt-calque");
		editJson(root, PT, (o) => void (o.app_save_button = "Realizar a ação"));
		expect(rulesOf(await writing(root))).toEqual(["writing-pt-calque"]);
	});

	test("word boundaries understand accents: 'realizar' does not fire inside a longer word", async () => {
		const root = await repo();
		editJson(root, PT, (o) => void (o.app_save_button = "Irrealizável e realizarão"));
		expect(rulesOf(await writing(root))).not.toContain("writing-pt-calque");
	});

	test("except drops a hit when the sentence matches", async () => {
		const root = await repo();
		editJson(root, EN, (o) => void (o.app_save_button = "Unlock the keychain"));
		expect(rulesOf(await writing(root))).toEqual([]);
		editJson(root, EN, (o) => void (o.app_save_button = "Unlock your potential"));
		expect(rulesOf(await writing(root))).toEqual(["writing-en-hype"]);
	});

	test("severity warn is a warning, and scope docs never fires on a catalog", async () => {
		const root = await repo();
		editJson(root, EN, (o) => void (o.app_save_button = "Simply save!"));
		const fs = await writing(root);
		expect(finding(fs, "writing-ui-bang")?.severity).toBe("warn");
		expect(rulesOf(fs)).not.toContain("writing-docs-only");
		expect(errors(fs)).toEqual([]);
	});

	test("flags 'u' makes a rule case-sensitive; the default flags ignore case", async () => {
		const root = await repo();
		editJson(root, EN, (o) => void (o.app_save_button = "allcaps then ALLCAPS"));
		expect((await writing(root)).filter((f) => f.rule === "writing-case-sensitive")).toHaveLength(1);
		editJson(root, EN, (o) => void (o.app_save_button = "allcaps"));
		expect(rulesOf(await writing(root))).toEqual([]);
		editJson(root, EN, (o) => void (o.app_save_button = "SEAMLESS"));
		expect(rulesOf(await writing(root))).toEqual(["writing-en-hype"]);
	});

	test("placeholders, markup tags, code spans and URLs are not text", async () => {
		const root = await repo();
		addKey(root, "app_probe", "Run `seamless` at https://seamless.example/{name} {#link}ok{/link}", "Execute `seamless` em https://seamless.example/{name} {#link}ok{/link}", "A probe");
		expect(await writing(root)).toEqual([]);
	});

	test("a description waives one rule when it gives a reason; without a reason the waiver itself is an error", async () => {
		const root = await repo();
		addKey(root, "app_dash", "A \u2014 B", "A \u2014 B", "Probe. writing-lint: allow em-dash: it is a literal range label");
		expect(await writing(root)).toEqual([]);
		editJson(root, META, (o) => void (o.app_dash = "Probe. writing-lint: allow em-dash"));
		const fs = await writing(root);
		expect(rulesOf(fs)).toContain("writing-allow");
		expect(rulesOf(fs)).toContain("writing-em-dash");
	});
});

describe("Markdown prose", () => {
	test("code, URLs, link targets, comments, tags and front matter are not prose; prose is, at its line", () => {
		const md = [
			"---",
			"title: A \u2014 B",
			"---",
			"",
			"Plain \u2014 dash.",
			"",
			"```sh",
      "echo \u2014 hi",
      "```",
			"",
			"`inline \u2014 code` and https://x.example/a\u2014b and [text](https://x.example/\u2014) and <!-- \u2014 -->",
			'<span title="\u2014">x</span>',
			"[ref]: https://x.example/\u2014",
		].join("\n");
		const prose = markdownProse(md);
		expect(prose.length).toBe(md.length);
		expect(prose.split("\n").map((l, i) => (l.includes("\u2014") ? i + 1 : 0)).filter(Boolean)).toEqual([5]);
	});

	test("allowsIn: a waiver needs a reason", () => {
		expect(allowsIn("<!-- writing-lint: allow em-dash: quoting a title -->")).toEqual({ ids: new Set(["em-dash"]), noReason: [] });
		expect(allowsIn("<!-- writing-lint: allow em-dash -->")).toEqual({ ids: new Set(), noReason: ["em-dash"] });
	});

	test("a translation under docs/<locale>/ is strict: the rule's severity, line and rule id", async () => {
		const root = await repo();
		put(root, "docs/pt-BR/getting-started.md", `${read(root, "docs/pt-BR/getting-started.md")}\nTexto \u2014 com travessão.\n\nOutra linha simply.\n`);
		const fs = (await writing(root)).filter((f) => f.file.startsWith("docs/pt-BR/"));
		expect(fs.map((f) => f.rule)).toEqual(expect.arrayContaining(["writing-em-dash", "writing-docs-only"]));
		expect(fs.every((f) => f.severity === "error" && (f.line ?? 0) > 10)).toBe(true);
	});

	test("an allow comment on the line before waives that line only, and a reasonless one is an error", async () => {
		const root = await repo();
		const base = read(root, "docs/pt-BR/getting-started.md");
		put(root, "docs/pt-BR/getting-started.md", `${base}\n<!-- writing-lint: allow em-dash: quoting a heading -->\nA \u2014 B\n\nC \u2014 D\n`);
		const fs = (await writing(root)).filter((f) => f.rule === "writing-em-dash");
		expect(fs).toHaveLength(1);
		expect(fs[0].message).toContain("\u2014");
		put(root, "docs/pt-BR/getting-started.md", `${base}\n<!-- writing-lint: allow em-dash -->\nA \u2014 B\n`);
		expect(rulesOf(await writing(root))).toContain("writing-allow");
	});

	test("the style guide and the rules file are never linted", async () => {
		const root = await repo();
		put(root, "i18n/style/README.md", "Never write \u2014 or seamless.\n");
		put(root, RULES_FILE, `${MINI}# \u2014 seamless \u2014\n`);
		expect(await writing(root)).toEqual([]);
	});
});

describe("existing prose is ratcheted, not failed", () => {
	const RM = "README.md";

	test("hits up to the baseline are tolerated; one more fails; fewer demands a shrink that fix does", async () => {
		const root = await repo();
		put(root, RM, "# Atlas\n\nOne \u2014 two.\n\nThree \u2014 four.\n");
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ writing: { [RM]: 2 } }));
		expect(errors(await check(root, "writing"))).toEqual([]);

		put(root, RM, "# Atlas\n\nOne \u2014 two.\n\nThree \u2014 four.\n\nFive \u2014 six.\n");
		const over = errors(await check(root, "writing"));
		expect(over).toHaveLength(3);
		expect(over.every((f) => f.file === RM && f.rule === "writing-em-dash")).toBe(true);
		expect(over[0].message).toContain("baseline 2");

		put(root, RM, "# Atlas\n\nOne \u2014 two.\n");
		const stale = finding(await check(root, "writing"), "writing-baseline");
		expect(stale).toMatchObject({ severity: "error", key: RM });
		expect(stale?.message).toContain("i18n:fix");
		fix(ctxOf(root));
		expect(readBaseline(root).writing?.[RM]).toBe(1);
		expect(errors(await check(root, "writing"))).toEqual([]);
	});

	test("a new English doc with no baseline entry fails on its first offender", async () => {
		const root = await repo();
		put(root, "docs/new-page.md", "# New\n\nIt is seamless.\n");
		expect(finding(await check(root, "writing"), "writing-en-hype")).toMatchObject({ severity: "error", file: "docs/new-page.md", line: 3 });
	});

	test("warning-level rules and tolerated hits are info, hidden by default and never failing", async () => {
		const root = await repo();
		put(root, RM, "# Atlas\n\nShouting ALLCAPS here.\n\nA \u2014 B\n");
		put(root, "i18n/hardcoded-baseline.json", JSON.stringify({ writing: { [RM]: 1 } }));
		const fs = await check(root, "writing");
		expect(fs.filter((f) => f.severity !== "info")).toEqual([]);
		expect(fs.filter((f) => f.severity === "info" && f.file === RM).length).toBe(2);
	});

	test("CHANGELOG, skills and site content are existing prose too", async () => {
		const root = await repo();
		put(root, "CHANGELOG.md", "# Changelog\n\n- fixed a thing \u2014 again\n");
		put(root, "skills/atlas/SKILL.md", "Use it \u2014 seamlessly.\n");
		const fs = errors(await check(root, "writing"));
		expect(fs.map((f) => f.file)).toEqual(expect.arrayContaining(["CHANGELOG.md", "skills/atlas/SKILL.md"]));
	});
});

describe("the shipped rules on real text", () => {
	const real = readFileSync(join(import.meta.dir, "..", "..", RULES_FILE), "utf8");

	test.each([
		["em dash", "Save \u2014 now", "writing-em-dash"],
		["en dash", "Pages 2\u20135", "writing-en-dash"],
		["emoji", "Done 🚀", "writing-emoji"],
		["hype", "A seamless install", "writing-en-seamless"],
		["AI vocabulary", "Let us delve into logs", "writing-en-ai-vocab-strong"],
		["negative parallelism", "Not just fast but safe", "writing-en-not-just-but"],
	])("English %s in a catalog value is an error", async (_n, text, rule) => {
		const root = await makeRepo({ [RULES_FILE]: real });
		addKey(root, "app_probe", text, text, "A probe");
		const f = finding(await check(root, "writing"), rule, "app_probe");
		expect(f?.severity).toBe("error");
	});

	test("the plain strings of a valid catalog pass the shipped rules", async () => {
		const root = await makeRepo({ [RULES_FILE]: real });
		expect(errors(await check(root, "writing"))).toEqual([]);
	});

	test("an exclamation mark at the end of a UI string warns", async () => {
		const root = await makeRepo({ [RULES_FILE]: real });
		addKey(root, "app_probe", "Saved!", "Salvo!", "A probe");
		expect(finding(await check(root, "writing"), "writing-exclamation-in-ui", "app_probe")?.severity).toBe("warn");
	});
});


// Incident: the rule's `\s+` crossed line breaks, so every Markdown list bullet after a prose line ("text\n- item")
// was reported as a spaced hyphen used as an em dash (200 false warnings on docs/*.md). The dash has to sit between
// two clauses on ONE line.
describe("ascii-dash-as-em-dash on Markdown", () => {
	const rules = loadRules(join(import.meta.dir, "..", "..")).rules;
	const dashes = (md: string, lang = "en") => lintMarkdown(rules, md, lang).hits.filter((h) => h.rule.id === "ascii-dash-as-em-dash").map((h) => h.line);

	test("list bullets, nested bullets and numbered items are not dashes", () => {
		const md = ["Requirements.", "- one thing", "- another thing", "  - nested thing", "    - deeper thing", "", "Next paragraph (see below)", "- after a paren", "1. first", "- mixed"].join("\n");
		expect(dashes(md)).toEqual([]);
	});

	test("table separator rows and cells are not dashes", () => {
		const md = ["| Command | What |", "|---|---|", "| a | b |", "", "| Command | What |", "| --- | :---: |", "| `x` | y |", "| a | - |"].join("\n");
		expect(dashes(md)).toEqual([]);
	});

	test("CLI options and ranges are not dashes", () => {
		expect(dashes("Run `atlas` with --json or pass --port 3 to it.\nWait 2-5 minutes, or use the build --release flag.\nSee pages 10-20.")).toEqual([]);
	});

	test("a spaced hyphen or double hyphen between two clauses is still flagged, at its line", () => {
		const md = ["Intro line.", "- a bullet", "", "Atlas starts fast - then it waits.", "", "It works -- mostly.", "- last bullet"].join("\n");
		expect(dashes(md)).toEqual([4, 6]);
		expect(dashes("Atlas starts fast - then waits.", "pt-BR")).toEqual([1]);
	});

	test("a clause ending a line and a bullet starting the next is not one dash", () => {
		expect(dashes("It ends here\n- and a bullet follows")).toEqual([]);
	});
});
