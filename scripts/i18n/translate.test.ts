// i18n:translate: what it sends, what it refuses to write, and that what it writes passes i18n:check.
// The model is the only fake (a function returning the answer); files, locks and every rule are the real ones.
import { afterAll, describe, expect, test } from "bun:test";
import { buildPrompt, type Complete, parseAnswer, pending, providerFromEnv, translateSurface } from "./translate.ts";
import { loadSurfaces } from "./surfaces.ts";
import { loadCtx } from "./types.ts";
import { addKey, check, cleanup, editJson, errors, makeRepo, PLURAL, put, read } from "./testkit.ts";
import { join } from "node:path";

afterAll(cleanup);

const EN = "apps/desktop/messages/en.json";
const PT = "apps/desktop/messages/pt-BR.json";
const LOCK = "i18n/lock/desktop/pt-BR/app.json";

const desktop = (root: string) => {
	const ctx = loadCtx(root);
	const loaded = loadSurfaces(ctx).find((l) => l.surface.name === "desktop");
	if (!loaded) throw new Error("no desktop surface");
	return { ctx, loaded };
};

/** A repo where one key is missing in pt-BR, one is outdated and one is new in English. */
async function dirtyRepo() {
	const root = await makeRepo();
	editJson(root, PT, (o) => void delete o.app_save_button);
	editJson(root, EN, (o) => void (o.app_welcome = "Welcome back to Atlas, {name}"));
	editJson(root, EN, (o) => void (o.app_new_thing = "New thing for {name}"));
	editJson(root, "apps/desktop/messages/descriptions.json", (o) => void (o.app_new_thing = "A new card title"));
	put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<i>{m.app_new_thing()}</i>\n`);
	return root;
}

const ANSWER = {
	app_save_button: "Salvar",
	app_welcome: "Bem-vindo de volta ao Atlas, {name}",
	app_new_thing: "Coisa nova para {name}",
};
const answering = (a: unknown): Complete => async () => JSON.stringify(a);

describe("what is pending", () => {
	test("missing and outdated keys only: not fresh ones, not a translation that merely has no lock entry", async () => {
		const root = await dirtyRepo();
		editJson(root, PT, (o) => void (o.app_unstamped = "Sem lock"));
		editJson(root, EN, (o) => void (o.app_unstamped = "No lock"));
		const { ctx, loaded } = desktop(root);
		const p = pending(ctx, loaded.surface, "pt-BR");
		expect(p.map((x) => x.key).sort()).toEqual(["app_new_thing", "app_save_button", "app_welcome"]);
		expect(p.find((x) => x.key === "app_welcome")?.previous?.raw).toBe(JSON.stringify("Bem-vindo ao Atlas, {name}"));
		expect(p.find((x) => x.key === "app_save_button")?.previous).toBeUndefined();
	});

	test("a clean repo has nothing to translate", async () => {
		const { ctx, loaded } = desktop(await makeRepo());
		expect(pending(ctx, loaded.surface, "pt-BR")).toEqual([]);
	});
});

describe("the prompt", () => {
	test("carries the description, the English, the previous translation and only the glossary entries that occur", async () => {
		const root = await dirtyRepo();
		const { ctx, loaded } = desktop(root);
		const batch = pending(ctx, loaded.surface, "pt-BR");
		const { system, user } = buildPrompt(ctx, loaded.surface, "pt-BR", batch);
		expect(user).toContain("description: Primary toolbar button that saves the open file");
		expect(user).toContain('en: "Save"');
		expect(user).toContain('previous pt-BR (the English changed since): "Bem-vindo ao Atlas, {name}"');
		expect(user).toContain("- Atlas: never translate, keep exactly");
		const glossary = user.slice(user.indexOf("Glossary:"), user.indexOf("Existing translations"));
		expect(glossary).not.toContain("branch"); // only Atlas occurs in this batch
		expect(glossary).not.toContain("settings ->");
		expect(system).toContain("pt-BR");
		expect(system).toContain("ONE JSON object");
	});

	test("a rendering entry appears when its term occurs", async () => {
		const root = await makeRepo();
		addKey(root, "app_open_settings", "Open settings", "Abrir configurações", "Menu item");
		editJson(root, EN, (o) => void (o.app_open_settings = "Open the settings"));
		const { ctx, loaded } = desktop(root);
		expect(buildPrompt(ctx, loaded.surface, "pt-BR", pending(ctx, loaded.surface, "pt-BR")).user).toContain("- settings -> configurações");
	});

	test("OS nouns, genders and advice from the glossary reach the prompt", async () => {
		const root = await makeRepo();
		put(root, "i18n/glossary.yaml", `- term: branch
  dnt: true
  gender: { pt-BR: f }
- term: run
  advice: { pt-BR: 'Button: Executar' }
- term: file manager
  os:
    en: { mac: Finder, windows: File Explorer, linux: the file manager }
    pt-BR: { mac: Finder, windows: Explorador de Arquivos, linux: gerenciador de arquivos }
- term: trash
  os:
    en: { mac: Trash, windows: Recycle Bin, linux: Trash }
    pt-BR: { mac: Lixeira, windows: Lixeira, linux: Lixeira }
`);
		addKey(root, "app_reveal", "Run it, then open the branch in Finder", "Rode, depois abra a branch no Finder", "A probe");
		editJson(root, EN, (o) => void (o.app_reveal = "Run it, then open the branch in the file manager"));
		const { ctx, loaded } = desktop(root);
		const user = buildPrompt(ctx, loaded.surface, "pt-BR", pending(ctx, loaded.surface, "pt-BR")).user;
		expect(user).toContain("- branch: never translate, keep exactly (gender: f)");
		expect(user).toContain("- run: Button: Executar");
		expect(user).toContain("- file manager (select on os): mac: Finder | windows: Explorador de Arquivos | linux: gerenciador de arquivos");
		expect(user).not.toContain("Lixeira"); // trash is not in the batch
	});

	test("the style guide of the target locale is included when the file exists, and only then", async () => {
		const root = await dirtyRepo();
		const { ctx, loaded } = desktop(root);
		const batch = pending(ctx, loaded.surface, "pt-BR");
		expect(buildPrompt(ctx, loaded.surface, "pt-BR", batch).user).not.toContain("Style guide");
		put(root, "i18n/style/pt-BR.md", "Use você. Buttons in the infinitive.\n");
		expect(buildPrompt(ctx, loaded.surface, "pt-BR", batch).user).toContain("Style guide for pt-BR:\nUse você. Buttons in the infinitive.");
	});

	test("includes at most 20 existing pairs of the same area, never the keys being translated", async () => {
		const root = await dirtyRepo();
		for (let i = 0; i < 25; i++) addKey(root, `app_extra_${String(i).padStart(2, "0")}`, `Extra ${i}`, `Extra ${i} pt`, `Extra number ${i}`);
		editJson(root, EN, (o) => void (o.app_new_thing = "New thing for {name}"));
		const { ctx, loaded } = desktop(root);
		const batch = pending(ctx, loaded.surface, "pt-BR");
		const user = buildPrompt(ctx, loaded.surface, "pt-BR", batch).user;
		const section = user.slice(user.indexOf("Existing translations"), user.indexOf("Translate these:"));
		expect(section.match(/^ {2}en: /gm)).toHaveLength(20);
		expect(section).not.toContain("app_new_thing");
	});

	test("an area with no fresh pairs adds no examples section", async () => {
		const root = await dirtyRepo();
		editJson(root, EN, (o) => void (o.zzz_alone = "Alone"));
		const { ctx, loaded } = desktop(root);
		const p = pending(ctx, loaded.surface, "pt-BR").filter((x) => x.key === "zzz_alone");
		expect(buildPrompt(ctx, loaded.surface, "pt-BR", p).user).not.toContain("Existing translations");
	});
});

describe("writing", () => {
	test("a valid answer is written canonically, stamped, and the repo passes i18n:check", async () => {
		const root = await dirtyRepo();
		const before = JSON.parse(read(root, PT));
		const { ctx, loaded } = desktop(root);
		const report = await translateSurface(ctx, loaded, "pt-BR", answering(ANSWER));
		expect(report.errors).toEqual([]);
		expect(report.translated.sort()).toEqual(["app_new_thing", "app_save_button", "app_welcome"]);
		const after = JSON.parse(read(root, PT));
		expect(after.app_save_button).toBe("Salvar");
		expect(after.app_welcome).toBe("Bem-vindo de volta ao Atlas, {name}");
		for (const k of ["app_files_count", "app_open_branch", "app_open_docs"]) expect(after[k]).toEqual(before[k]);
		expect(Object.keys(after)).toEqual([...Object.keys(after)].sort());
		expect(errors(await check(root))).toEqual([]);
		const again = desktop(root);
		expect(pending(again.ctx, again.loaded.surface, "pt-BR")).toEqual([]);
	});

	test("one invalid key (a dropped {name}) means NOTHING is written: catalog and lock stay byte-identical", async () => {
		const root = await dirtyRepo();
		const [catalog, lock] = [read(root, PT), read(root, LOCK)];
		const { ctx, loaded } = desktop(root);
		const report = await translateSurface(ctx, loaded, "pt-BR", answering({ ...ANSWER, app_new_thing: "Coisa nova" }));
		expect(report.translated).toEqual([]);
		expect(report.errors).toHaveLength(1);
		expect(report.errors[0]).toContain("app_new_thing");
		expect(report.errors[0]).toContain("{name}");
		expect(read(root, PT)).toBe(catalog);
		expect(read(root, LOCK)).toBe(lock);
	});

	test.each([
		["a pt-PT word", { ...ANSWER, app_save_button: "Guardar" }, "pt-pt"],
		["a translated do-not-translate term", { ...ANSWER, app_welcome: "Bem-vindo de volta, {name}" }, "dnt"],
		["a missing key", { app_save_button: "Salvar", app_welcome: ANSWER.app_welcome }, "missing in the model answer"],
		["a malformed variants value", { ...ANSWER, app_save_button: [{ match: 1 }] }, "match"],
		["an empty string", { ...ANSWER, app_save_button: "" }, "empty"],
	])("%s is refused and nothing is written", async (_n, answer, text) => {
		const root = await dirtyRepo();
		const before = read(root, PT);
		const { ctx, loaded } = desktop(root);
		const report = await translateSurface(ctx, loaded, "pt-BR", answering(answer));
		expect(report.errors.join("\n")).toContain(text);
		expect(read(root, PT)).toBe(before);
	});

	test("an answer that is not JSON is an error, not a crash, and writes nothing", async () => {
		const root = await dirtyRepo();
		const before = read(root, PT);
		const { ctx, loaded } = desktop(root);
		const report = await translateSurface(ctx, loaded, "pt-BR", async () => "Sorry, I cannot do that.");
		expect(report.errors[0]).toContain("no JSON object");
		expect(read(root, PT)).toBe(before);
	});

	test("plural variants come back as a variants array and must have one and other", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_files_count);
		const { ctx, loaded } = desktop(root);
		const bad = await translateSurface(ctx, loaded, "pt-BR", answering({ app_files_count: [{ ...PLURAL("", "")[0], match: { "countPlural=*": "{count} arquivos" } }] }));
		expect(bad.errors.join("\n")).toContain("missing one");
		const ok = await translateSurface(ctx, loaded, "pt-BR", answering({ app_files_count: PLURAL("{count} arquivo", "{count} arquivos") }));
		expect(ok.errors).toEqual([]);
		expect(errors(await check(root))).toEqual([]);
	});

	test("a code fence around the JSON is tolerated", () => {
		expect(parseAnswer('Here you go:\n```json\n{ "a": "b" }\n```\n')).toEqual({ a: "b" });
	});
});

describe("Fluent (Rust) surface", () => {
	const PTF = "crates/atlas-i18n/locales/pt-BR/cli.ftl";
	const rust = (root: string) => {
		const ctx = loadCtx(root);
		const loaded = loadSurfaces(ctx).find((l) => l.surface.name === "rust");
		if (!loaded) throw new Error("no rust surface");
		return { ctx, loaded };
	};

	test("a missing message is appended and an outdated one replaced in place; the rest of the file is untouched", async () => {
		const root = await makeRepo();
		put(root, PTF, read(root, PTF).replace(/\ncli-files-count[\s\S]*$/, "\n"));
		put(root, "crates/atlas-i18n/locales/en/cli.ftl", read(root, "crates/atlas-i18n/locales/en/cli.ftl").replace("Language: { $lang }", "Current language: { $lang }"));
		const { ctx, loaded } = rust(root);
		const files = "cli-files-count =\n    { $count ->\n        [0] Nenhum arquivo\n        [one] { $count } arquivo\n       *[other] { $count } arquivos\n    }";
		const report = await translateSurface(ctx, loaded, "pt-BR", answering({ "cli-files-count": files, "cli-lang-current": "cli-lang-current = Idioma atual: { $lang }" }));
		expect(report.errors).toEqual([]);
		const text = read(root, PTF);
		expect(text).toContain("cli-lang-current = Idioma atual: { $lang }");
		expect(text).not.toContain("Idioma: {");
		expect(text).toContain("[0] Nenhum arquivo");
		expect(text.match(/cli-lang-current/g)).toHaveLength(1);
		expect(errors(await check(root, "rust"))).toEqual([]);
	});

	test.each([
		["a wrong id", { "cli-files-count": "cli-other = x" }, "expected"],
		["Fluent junk", { "cli-files-count": "cli-files-count = { $count" }, "syntax"],
		["a missing variable", { "cli-files-count": "cli-files-count = arquivos" }, "count"],
		["a non-string", { "cli-files-count": 3 }, "string"],
	])("%s is refused and nothing is written", async (_n, answer, text) => {
		const root = await makeRepo();
		put(root, PTF, read(root, PTF).replace(/\ncli-files-count[\s\S]*$/, "\n"));
		const before = read(root, PTF);
		const { ctx, loaded } = rust(root);
		const report = await translateSurface(ctx, loaded, "pt-BR", answering(answer));
		expect(report.errors.join("\n")).toContain(text);
		expect(read(root, PTF)).toBe(before);
	});
});

describe("bun scripts/i18n/translate.ts", () => {
	const run = async (root: string, args: string[], env: Record<string, string> = {}) => {
		const p = Bun.spawn(["bun", join(import.meta.dir, "translate.ts"), "--root", root, ...args], {
			stdout: "pipe",
			stderr: "pipe",
			env: { ...process.env, ANTHROPIC_API_KEY: "", I18N_BASE_URL: "", I18N_MODEL: "", ...env },
		});
		const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
		return { out, err, code };
	};

	test("--dry-run prints the prompt, needs no key or network, and writes nothing", async () => {
		const root = await dirtyRepo();
		const [catalog, lock] = [read(root, PT), read(root, LOCK)];
		const r = await run(root, ["--dry-run"]);
		expect(r.code).toBe(0);
		expect(r.out).toContain("=== desktop pt-BR (batch 1) ===");
		expect(r.out).toContain("description: Primary toolbar button that saves the open file");
		expect(read(root, PT)).toBe(catalog);
		expect(read(root, LOCK)).toBe(lock);
	});

	test("without --dry-run and no model configured it fails with what to set, writing nothing", async () => {
		const root = await dirtyRepo();
		const before = read(root, PT);
		const r = await run(root, []);
		expect(r.code).toBe(1);
		expect(r.err).toContain("I18N_MODEL");
		expect(read(root, PT)).toBe(before);
	});

	test("a clean repo says there is nothing to translate and needs no model", async () => {
		const r = await run(await makeRepo(), []);
		expect(r.code).toBe(0);
		expect(r.out).toContain("nothing to translate");
	});

	test("an unknown --locale fails", async () => {
		const r = await run(await makeRepo(), ["--locale", "xx"]);
		expect(r.code).toBe(1);
	});

	test("an OpenAI-compatible endpoint (I18N_BASE_URL) gets the system and user prompt and its answer is written", async () => {
		const root = await dirtyRepo();
		let seen: { auth: string | null; body: { model: string; messages: { role: string; content: string }[] } } | undefined;
		const server = Bun.serve({
			port: 0,
			async fetch(req) {
				seen = { auth: req.headers.get("authorization"), body: await req.json() };
				return Response.json({ choices: [{ message: { content: JSON.stringify(ANSWER) } }] });
			},
		});
		try {
			const r = await run(root, [], { I18N_BASE_URL: `http://127.0.0.1:${server.port}/v1`, I18N_MODEL: "local-model", I18N_API_KEY: "k-123" });
			expect(r.code).toBe(0);
			expect(seen?.auth).toBe("Bearer k-123");
			expect(seen?.body.model).toBe("local-model");
			expect(seen?.body.messages.map((m) => m.role)).toEqual(["system", "user"]);
			expect(seen?.body.messages[1].content).toContain("app_new_thing");
			expect(JSON.parse(read(root, PT)).app_save_button).toBe("Salvar");
			expect(errors(await check(root))).toEqual([]);
		} finally {
			server.stop(true);
		}
	});
});

describe("providerFromEnv", () => {
	test("needs a model and a credential or endpoint", () => {
		expect(() => providerFromEnv({})).toThrow("I18N_MODEL");
		expect(() => providerFromEnv({ I18N_MODEL: "m" })).toThrow("ANTHROPIC_API_KEY");
		expect(() => providerFromEnv({ I18N_MODEL: "m", ANTHROPIC_API_KEY: "x" })).not.toThrow();
		expect(() => providerFromEnv({ I18N_MODEL: "m", I18N_BASE_URL: "http://localhost:11434/v1" })).not.toThrow();
	});
});
