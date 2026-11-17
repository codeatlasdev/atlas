// Every rule of the Paraglide checker, red on a bad catalog and green on the repaired one. The repo under test is
// built in a temp dir by testkit.ts (valid, locks stamped by the real `i18n:fix`); each test breaks one thing.
import { afterAll, describe, expect, test } from "bun:test";
import { rmSync } from "node:fs";
import { fix } from "./fix.ts";
import { type Finding, loadCtx } from "./types.ts";
import { EN, META, PT, addKey, check, cleanup, editJson, errors, finding, json, makeRepo, PLURAL, put, read, rulesOf } from "./testkit.ts";

afterAll(cleanup);

const DESC = META;

describe("a valid repo", () => {
	test("has no error and no warning", async () => {
		const root = await makeRepo();
		expect((await check(root)).filter((f) => f.severity !== "info")).toEqual([]);
	});
});

describe("missing / outdated / orphan / lock", () => {
	test("a key the required locale lacks is an error naming the key and the catalog; a community locale only warns", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_save_button);
		const f = finding(await check(root), "missing", "app_save_button");
		expect(f).toMatchObject({ severity: "error", file: PT });
		put(root, "i18n/locales.json", json({ default: "en", locales: { en: { tier: "source" }, "pt-BR": { tier: "community" } } }));
		expect(finding(await check(root), "missing", "app_save_button")?.severity).toBe("warn");
	});

	test("an empty translation counts as missing text", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_save_button = ""));
		expect(rulesOf(errors(await check(root)))).toContain("missing");
	});

	test("changing the English makes exactly that key outdated, and fix does not hide it; --restamp does", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_save_button = "Save file"));
		const outdated = (fs: Finding[]) => fs.filter((f) => f.rule === "outdated").map((f) => f.key);
		expect(outdated(await check(root))).toEqual(["app_save_button"]);
		fix(loadCtx(root));
		expect(outdated(await check(root))).toEqual(["app_save_button"]);
		fix(loadCtx(root), { restamp: ["app_save_button"] });
		expect(errors(await check(root))).toEqual([]);
	});

	test("a description edit never outdates a translation (the lock hashes the text only)", async () => {
		const root = await makeRepo();
		editJson(root, DESC, (o) => void (o.app_save_button = "Toolbar button that saves the open file to disk"));
		expect(errors(await check(root))).toEqual([]);
	});

	test("a translation without a lock entry is unstamped until fix stamps it", async () => {
		const root = await makeRepo();
		rmSync(`${root}/i18n/messages/app/lock.json`);
		const fs = await check(root);
		expect(fs.filter((f) => f.rule === "unstamped")).toHaveLength(5);
		fix(loadCtx(root));
		expect(errors(await check(root))).toEqual([]);
	});

	test("a key only the translation has is an orphan, even in a community locale", async () => {
		const root = await makeRepo();
		put(root, "i18n/locales.json", json({ default: "en", locales: { en: { tier: "source" }, "pt-BR": { tier: "community" } } }));
		editJson(root, PT, (o) => void (o.app_gone = "Sumiu"));
		expect(finding(await check(root), "orphan", "app_gone")?.severity).toBe("error");
	});

	test("a stale lock entry (key deleted everywhere) is an error that fix removes", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void delete o.app_welcome);
		editJson(root, PT, (o) => void delete o.app_welcome);
		editJson(root, DESC, (o) => void delete o.app_welcome);
		expect(finding(await check(root), "lock", "app_welcome")).toBeDefined();
		fix(loadCtx(root));
		expect((await check(root)).filter((f) => f.rule === "lock")).toEqual([]);
	});

	test("a locale file i18n/locales.json does not know is a config error", async () => {
		const root = await makeRepo();
		put(root, "i18n/messages/app/es.json", "{}\n");
		expect(finding(await check(root), "config")?.message).toContain("not a locale");
	});
});

describe("lock.json is only ever written by the tools", () => {
	const LOCKF = "i18n/messages/app/lock.json";
	test("a hand-edited lock (unsorted, other indent), a bad hash, a wrong shape and invalid JSON are lock errors; fix repairs the first", async () => {
		const root = await makeRepo();
		const lock = JSON.parse(read(root, LOCKF));
		const keys = Object.keys(lock["pt-BR"]);
		const reversed = Object.fromEntries([...keys].reverse().map((k) => [k, lock["pt-BR"][k]]));
		put(root, LOCKF, `${JSON.stringify({ "pt-BR": reversed }, null, "\t")}\n`);
		expect(finding(await check(root), "lock")?.message).toContain("not in canonical form");
		fix(loadCtx(root));
		expect(rulesOf(await check(root))).not.toContain("lock");

		put(root, LOCKF, `${JSON.stringify({ "pt-BR": { ...lock["pt-BR"], app_welcome: "nothex" } }, null, "  ")}\n`);
		expect((await check(root)).some((f) => f.rule === "lock" && f.key === "app_welcome" && f.message.includes("12-hex"))).toBe(true);
		put(root, LOCKF, '{ "pt-BR": ["a"] }\n');
		expect(finding(await check(root), "lock")?.message).toContain('{ "<locale>": { "<key>": "<hash>" } }');
		put(root, LOCKF, "{ nope");
		expect(finding(await check(root), "lock")?.message).toContain("invalid JSON");
	});

	test("a lock of one locale never touches the others: locales share the file", async () => {
		const root = await makeRepo();
		put(root, LOCKF, `${JSON.stringify({ "es": { app_welcome: "0123456789ab" }, "pt-BR": JSON.parse(read(root, LOCKF))["pt-BR"] }, null, "  ")}\n`);
		fix(loadCtx(root));
		expect(JSON.parse(read(root, LOCKF)).es.app_welcome).toBe("0123456789ab");
	});
});

describe("unused", () => {
	test("a key no source references is unused; an i18n-keys annotation covers dynamic use", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_dead_one = "Dead"));
		editJson(root, PT, (o) => void (o.app_dead_one = "Morta"));
		editJson(root, DESC, (o) => void (o.app_dead_one = "Nothing renders this"));
		fix(loadCtx(root));
		expect(finding(await check(root), "unused", "app_dead_one")).toMatchObject({ severity: "error", file: EN });
		put(root, "apps/desktop/src/dynamic.ts", "// i18n-keys: app_dead_*\nexport const k = (n: string) => m[`app_dead_${n}`];\n");
		expect(rulesOf(await check(root))).not.toContain("unused");
	});

	test("m.key, m['key'], messages_key and a named paraglide import all count as references", async () => {
		const root = await makeRepo();
		for (const k of ["app_a_one", "app_b_two", "app_c_three", "app_d_four"]) {
			editJson(root, EN, (o) => void (o[k] = `T ${k}`));
			editJson(root, PT, (o) => void (o[k] = `X ${k}`));
			editJson(root, DESC, (o) => void (o[k] = `where ${k} shows`));
		}
		put(root, "apps/desktop/src/refs.ts", `import { app_d_four } from "$lib/paraglide/messages.js";\nm["app_b_two"]; messages_app_c_three; m.app_a_one; app_d_four();\n`);
		fix(loadCtx(root));
		expect(rulesOf(await check(root))).not.toContain("unused");
	});

	test("a reference inside the generated paraglide output does not count", async () => {
		const root = await makeRepo();
		addKey(root, "app_x_key", "X", "Y", "Used only by generated code");
		put(root, "apps/desktop/src/App.svelte", read(root, "apps/desktop/src/App.svelte").replace("<i>{m.app_x_key()}</i>\n", ""));
		put(root, "apps/desktop/src/lib/paraglide/messages.js", "export const app_x_key = () => 'X'; m.app_x_key;\n");
		expect(finding(await check(root), "unused", "app_x_key")).toBeDefined();
	});
});

describe("placeholders, plurals, verbatim", () => {
	test("a dropped, renamed or extra variable is a placeholder error", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_welcome = "Bem-vindo ao Atlas"));
		expect(finding(await check(root), "placeholder", "app_welcome")?.message).toContain("{name}");
		editJson(root, PT, (o) => void (o.app_welcome = "Bem-vindo ao Atlas, {nome}"));
		const msgs = (await check(root)).filter((f) => f.rule === "placeholder").map((f) => f.message);
		expect(msgs.some((m) => m.includes("{name}") && m.includes("missing"))).toBe(true);
		expect(msgs.some((m) => m.includes("{nome}") && m.includes("not in the source"))).toBe(true);
		editJson(root, PT, (o) => void (o.app_welcome = "Bem-vindo ao Atlas, {name}"));
		expect(errors(await check(root))).toEqual([]);
	});

	test("a lost markup tag is a placeholder error", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_open_docs = "Leia a documentação"));
		expect(finding(await check(root), "placeholder", "app_open_docs")?.message).toContain("{#link}");
	});

	test("pt-BR plural needs one and other; a plain string for a plural is a selector mismatch", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_files_count = [{ ...PLURAL("", "{count} arquivos")[0], match: { "countPlural=*": "{count} arquivos" } }]));
		expect(finding(await check(root), "plural", "app_files_count")?.message).toContain("missing one");
		editJson(root, PT, (o) => void (o.app_files_count = "{count} arquivos"));
		expect(finding(await check(root), "plural", "app_files_count")?.message).toContain("selector");
		editJson(root, PT, (o) => void (o.app_files_count = PLURAL("{count} arquivo", "{count} arquivos")));
		expect(errors(await check(root))).toEqual([]);
	});

	test("an English plural without `one` is itself an error", async () => {
		const root = await makeRepo();
		editJson(root, EN, (o) => void (o.app_files_count = [{ ...PLURAL("", "")[0], match: { "countPlural=*": "{count} files" } }]));
		expect(rulesOf(errors(await check(root)))).toContain("plural");
	});

	test("an explicit zero variant is allowed next to one and other (\"0 arquivo\" reads wrong)", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => {
			const v = PLURAL("{count} arquivo", "{count} arquivos");
			v[0].match["countPlural=0" as "countPlural=one"] = "Nenhum arquivo";
			o.app_files_count = v;
		});
		fix(loadCtx(root));
		expect(errors(await check(root))).toEqual([]);
	});

	test("a changed code span or URL is a verbatim error", async () => {
		const root = await makeRepo();
		addKey(root, "app_run_hint", "Run `atlas dev` or see https://atlas.codeatlas.com.br", "Execute `atlas dev` ou veja https://atlas.codeatlas.com.br", "Hint under the empty state");
		expect(errors(await check(root))).toEqual([]);
		editJson(root, PT, (o) => void (o.app_run_hint = "Execute `atlas run` ou veja https://example.com"));
		const msgs = (await check(root)).filter((f) => f.rule === "verbatim").map((f) => f.message);
		expect(msgs).toHaveLength(4);
	});
});

describe("descriptions and key names", () => {
	test("a source key without a description is an error", async () => {
		const root = await makeRepo();
		editJson(root, DESC, (o) => void delete o.app_save_button);
		expect(finding(await check(root), "description", "app_save_button")?.severity).toBe("error");
	});

	test("a description that only repeats the text is an error", async () => {
		const root = await makeRepo();
		editJson(root, DESC, (o) => void (o.app_save_button = "save"));
		expect(finding(await check(root), "description", "app_save_button")?.message).toContain("repeats");
	});

	test("a description of a key that does not exist is an error", async () => {
		const root = await makeRepo();
		editJson(root, DESC, (o) => void (o.app_nope = "Nothing"));
		expect(finding(await check(root), "description", "app_nope")).toBeDefined();
	});

	test("keys must be area_snake_name", async () => {
		const root = await makeRepo();
		for (const bad of ["Save", "save", "app-save"]) {
			editJson(root, EN, (o) => void (o[bad] = "X"));
			expect(finding(await check(root), "key", bad)).toBeDefined();
			editJson(root, EN, (o) => void delete o[bad]);
		}
	});
});

describe("glossary, do-not-translate, pt-PT", () => {
	test("a do-not-translate term missing from the translation is an error; the description can waive a homograph", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_open_branch = "Abrir ramo"));
		expect(finding(await check(root), "dnt", "app_open_branch")).toMatchObject({ severity: "error" });
		editJson(root, DESC, (o) => void (o.app_open_branch = "Menu item that opens the branch picker. i18n-allow dnt:branch"));
		expect(rulesOf(await check(root))).not.toContain("dnt");
	});

	test("Atlas is matched with its exact case; a lowercase term matches any case", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void (o.app_welcome = "Bem-vindo, {name}"));
		expect(finding(await check(root), "dnt", "app_welcome")?.message).toContain('"Atlas"');
		editJson(root, PT, (o) => void (o.app_welcome = "Bem-vindo ao ATLAS, {name}"));
		expect(rulesOf(await check(root))).toContain("dnt"); // ATLAS is not Atlas
		editJson(root, PT, (o) => void (o.app_open_branch = "Abrir Branch"));
		expect((await check(root)).filter((f) => f.rule === "dnt" && f.key === "app_open_branch")).toEqual([]);
	});

	test("a glossary rendering that is not used is a warning, not an error", async () => {
		const root = await makeRepo();
		addKey(root, "app_open_settings", "Open settings", "Abrir ajustes", "Menu item that opens the settings window");
		const f = finding(await check(root), "glossary", "app_open_settings");
		expect(f?.severity).toBe("warn");
		expect(f?.message).toContain("configurações");
		editJson(root, PT, (o) => void (o.app_open_settings = "Abrir configurações"));
		expect(rulesOf(await check(root))).not.toContain("glossary");
	});

	test.each([
		["Guardar", "salvar"],
		["Eliminar o ficheiro", "excluir"],
		["O utilizador está a gravar", "usuário"],
		["Abrir as definições", "configurações"],
		["Partilhar o ecrã", "compartilhar"],
		["Introduza a palavra-passe", "senha"],
	])("pt-PT %p is an error that names the pt-BR word", async (text, use) => {
		const root = await makeRepo();
		addKey(root, "app_pt_probe", "Probe", text, "A probe");
		const hits = (await check(root)).filter((f) => f.rule === "pt-pt" && f.key === "app_pt_probe");
		expect(hits.length).toBeGreaterThan(0);
		expect(hits.every((f) => f.severity === "error")).toBe(true);
		expect(hits.map((f) => f.message).join(" ")).toContain(use);
	});

	test("pt-BR wording that merely resembles pt-PT passes, and a waiver in the description is honoured", async () => {
		const root = await makeRepo();
		addKey(root, "app_pt_ok", "Probe", "Está gravando o registro do arquivo", "A probe");
		expect(rulesOf(await check(root))).not.toContain("pt-pt");
		editJson(root, PT, (o) => void (o.app_pt_ok = "Transferir"));
		expect(rulesOf(await check(root))).toContain("pt-pt");
		editJson(root, DESC, (o) => void (o.app_pt_ok = "Bank transfer. i18n-allow pt-pt:transferir"));
		expect(rulesOf(await check(root))).not.toContain("pt-pt");
	});

	test("the pt-PT list does not apply to other locales", async () => {
		const root = await makeRepo();
		put(root, "i18n/locales.json", json({ default: "en", locales: { en: { tier: "source" }, "pt-BR": { tier: "required" }, "pt-PT": { tier: "community" } } }));
		editJson(root, "apps/desktop/project.inlang/settings.json", (o) => void (o.locales = ["en", "pt-BR", "pt-PT"]));
		const ptpt = JSON.parse(read(root, PT));
		ptpt.app_save_button = "Guardar";
		put(root, "apps/desktop/messages/pt-PT.json", json(ptpt));
		fix(loadCtx(root));
		expect(rulesOf(await check(root))).not.toContain("pt-pt");
		editJson(root, PT, (o) => void (o.app_save_button = "Guardar"));
		expect(rulesOf(await check(root))).toContain("pt-pt");
	});
});

describe("catalog format", () => {
	test("unsorted keys, CRLF and a missing trailing newline are format errors that fix repairs", async () => {
		const root = await makeRepo();
		const obj = JSON.parse(read(root, PT));
		const reversed = Object.fromEntries(Object.entries(obj).reverse());
		for (const [text, why] of [
			[`${JSON.stringify(reversed, null, "\t")}\n`, "not sorted"],
			[read(root, PT).replaceAll("\n", "\r\n"), "CRLF"],
			[read(root, PT).trimEnd(), "trailing newline"],
		] as const) {
			put(root, PT, text);
			expect(finding(await check(root), "format")?.message).toContain(why);
			fix(loadCtx(root));
			expect(rulesOf(await check(root))).not.toContain("format");
			expect(read(root, PT)).toBe(json(obj));
		}
	});

	test("invalid JSON and a malformed variants array are catalog errors, not crashes", async () => {
		const root = await makeRepo();
		put(root, PT, "{ nope");
		expect(rulesOf(await check(root))).toContain("catalog");
		put(root, PT, json({ app_save_button: [{ match: "Salvar" }] }));
		expect(finding(await check(root), "catalog", "app_save_button")).toBeDefined();
	});

	test("fix keeps the indentation a catalog already uses", async () => {
		const root = await makeRepo();
		const text = JSON.stringify(JSON.parse(read(root, EN)), null, 2).replace(/\n/g, "\r\n");
		put(root, EN, text);
		fix(loadCtx(root));
		expect(read(root, EN)).toBe(`${JSON.stringify(JSON.parse(text), null, 2)}\n`);
	});
});
