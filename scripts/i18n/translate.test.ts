// i18n:translate: what it sends, what it refuses to write, and that what it writes passes i18n:check.
// The model is the only fake (a function returning the answer); files, locks and every rule are the real ones.
import { afterAll, describe, expect, test } from "bun:test";
import { join } from "node:path";
import { type Complete, buildPrompt, parseAnswer, pending, providerFromEnv, translateArea } from "./translate.ts";
import { loadSource } from "./source.ts";
import { loadCtx } from "./types.ts";
import { EN, META, OS, PLURAL, PT, addKey, addKeys, check, cleanup, editJson, errors, makeRepo, put, read } from "./testkit.ts";

afterAll(cleanup);

const LOCK = "i18n/messages/app/lock.json";

const app = (root: string, name = "app") => {
	const ctx = loadCtx(root);
	const source = loadSource(ctx);
	const area = source.areas.get(name);
	if (!area) throw new Error(`no area ${name}`);
	return { ctx, source, area };
};

/** A repo where one key is missing in pt-BR, one is outdated and one is new in English. */
async function dirtyRepo() {
	const root = await makeRepo();
	editJson(root, PT, (o) => void delete o.app_save_button);
	editJson(root, EN, (o) => void (o.app_welcome = "Welcome back to Atlas, {name}"));
	editJson(root, EN, (o) => void (o.app_new_thing = "New thing for {name}"));
	editJson(root, META, (o) => void (o.app_new_thing = "A new card title"));
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
		const { ctx, area } = app(root);
		const p = pending(ctx, area, "pt-BR");
		expect(p.map((x) => x.key).sort()).toEqual(["app_new_thing", "app_save_button", "app_welcome"]);
		expect(p.find((x) => x.key === "app_welcome")?.previous?.raw).toBe(JSON.stringify("Bem-vindo ao Atlas, {name}"));
		expect(p.find((x) => x.key === "app_save_button")?.previous).toBeUndefined();
	});

	test("a clean repo has nothing to translate", async () => {
		const { ctx, area } = app(await makeRepo());
		expect(pending(ctx, area, "pt-BR")).toEqual([]);
	});
});

describe("the prompt", () => {
	test("carries the description, the English, the previous translation and only the glossary entries that occur", async () => {
		const root = await dirtyRepo();
		const { ctx, area } = app(root);
		const batch = pending(ctx, area, "pt-BR");
		const { system, user } = buildPrompt(ctx, area, "pt-BR", batch);
		expect(user).toContain("description: Primary toolbar button that saves the open file");
		expect(user).toContain('en: "Save"');
		expect(user).toContain('previous pt-BR (the English changed since): "Bem-vindo ao Atlas, {name}"');
		expect(user).toContain("- Atlas: never translate, keep exactly");
		const glossary = user.slice(user.indexOf("Glossary:"), user.indexOf("Existing translations"));
		expect(glossary).not.toContain("branch"); // only Atlas occurs in this batch
		expect(glossary).not.toContain("settings ->");
		expect(system).toContain("pt-BR");
		expect(system).toContain("ONE JSON object");
		expect(system).toContain("{@reference}");
	});

	test("a rendering entry appears when its term occurs", async () => {
		const root = await makeRepo();
		addKey(root, "app_open_settings", "Open settings", "Abrir configurações", "Menu item");
		editJson(root, EN, (o) => void (o.app_open_settings = "Open the settings"));
		const { ctx, area } = app(root);
		expect(buildPrompt(ctx, area, "pt-BR", pending(ctx, area, "pt-BR")).user).toContain("- settings -> configurações");
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
		addKey(root, "app_reveal", OS("Open the branch in Finder", "Open the branch in File Explorer", "Open the branch in the file manager"), OS("Abra a branch no Finder", "Abra a branch no Explorador de Arquivos", "Abra a branch no gerenciador de arquivos"), "A probe. Run it");
		editJson(root, EN, (o) => void (o.app_reveal = OS("Run it, then open the branch in Finder", "Run it, then open the branch in File Explorer", "Run it, then open the branch in the file manager")));
		const { ctx, area } = app(root);
		const user = buildPrompt(ctx, area, "pt-BR", pending(ctx, area, "pt-BR")).user;
		expect(user).toContain("- branch: never translate, keep exactly (gender: f)");
		expect(user).toContain("- run: Button: Executar");
		expect(user).toContain("- file manager (select on os): mac: Finder | windows: Explorador de Arquivos | linux: gerenciador de arquivos");
		expect(user).not.toContain("Lixeira"); // trash is not in the batch
	});

	test("the style guide of the target locale is included when the file exists, and only then", async () => {
		const root = await dirtyRepo();
		const { ctx, area } = app(root);
		const batch = pending(ctx, area, "pt-BR");
		expect(buildPrompt(ctx, area, "pt-BR", batch).user).not.toContain("Style guide");
		put(root, "i18n/style/pt-BR.md", "Use você. Buttons in the infinitive.\n");
		expect(buildPrompt(ctx, area, "pt-BR", batch).user).toContain("Style guide for pt-BR:\nUse você. Buttons in the infinitive.");
	});

	test("includes at most 20 existing pairs of the same area, never the keys being translated", async () => {
		const root = await dirtyRepo();
		addKeys(root, Array.from({ length: 25 }, (_, i) => [`app_extra_${String(i).padStart(2, "0")}`, `Extra ${i}`, `Extra ${i} pt`, `Extra number ${i}`]));
		editJson(root, EN, (o) => void (o.app_new_thing = "New thing for {name}"));
		const { ctx, area } = app(root);
		const batch = pending(ctx, area, "pt-BR");
		const user = buildPrompt(ctx, area, "pt-BR", batch).user;
		const section = user.slice(user.indexOf("Existing translations"), user.indexOf("Translate these:"));
		expect(section.match(/^ {2}en: /gm)).toHaveLength(20);
		expect(section).not.toContain("app_new_thing");
	});

	test("examples come from the area being translated only", async () => {
		const root = await makeRepo();
		editJson(root, area0("cli", "en"), (o) => void (o.cli_new = "A new line"));
		editJson(root, area0("cli", "meta"), (o) => void (o.cli_new = "Something printed by the CLI"));
		const { ctx, area } = app(root, "cli");
		const user = buildPrompt(ctx, area, "pt-BR", pending(ctx, area, "pt-BR")).user;
		expect(user).toContain("Existing translations of the same area");
		expect(user).toContain("cli_lang_current");
		expect(user).not.toContain("app_save_button");
	});
});

const area0 = (name: string, file: string) => `i18n/messages/${name}/${file}.json`;

describe("writing", () => {
	test("a valid answer is written canonically, stamped, and the repo passes i18n:check", async () => {
		const root = await dirtyRepo();
		const before = JSON.parse(read(root, PT));
		const { ctx, source, area } = app(root);
		const report = await translateArea(ctx, source, area, "pt-BR", answering(ANSWER));
		expect(report.errors).toEqual([]);
		expect(report.translated.sort()).toEqual(["app_new_thing", "app_save_button", "app_welcome"]);
		const after = JSON.parse(read(root, PT));
		expect(after.app_save_button).toBe("Salvar");
		expect(after.app_welcome).toBe("Bem-vindo de volta ao Atlas, {name}");
		for (const k of ["app_files_count", "app_open_branch", "app_open_docs"]) expect(after[k]).toEqual(before[k]);
		expect(Object.keys(after)).toEqual([...Object.keys(after)].sort());
		expect(errors(await check(root))).toEqual([]);
		const again = app(root);
		expect(pending(again.ctx, again.area, "pt-BR")).toEqual([]);
	});

	test("one invalid key (a dropped {name}) means NOTHING is written: catalog and lock stay byte-identical", async () => {
		const root = await dirtyRepo();
		const [catalog, lock] = [read(root, PT), read(root, LOCK)];
		const { ctx, source, area } = app(root);
		const report = await translateArea(ctx, source, area, "pt-BR", answering({ ...ANSWER, app_new_thing: "Coisa nova" }));
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
		["a reference to a message that does not exist", { ...ANSWER, app_save_button: "Salvar {@common_nothing}" }, "dangling-ref"],
	])("%s is refused and nothing is written", async (_n, answer, text) => {
		const root = await dirtyRepo();
		const before = read(root, PT);
		const { ctx, source, area } = app(root);
		const report = await translateArea(ctx, source, area, "pt-BR", answering(answer));
		expect(report.errors.join("\n")).toContain(text);
		expect(read(root, PT)).toBe(before);
	});

	test("an answer that is not JSON is an error, not a crash, and writes nothing", async () => {
		const root = await dirtyRepo();
		const before = read(root, PT);
		const { ctx, source, area } = app(root);
		const report = await translateArea(ctx, source, area, "pt-BR", async () => "Sorry, I cannot do that.");
		expect(report.errors[0]).toContain("no JSON object");
		expect(read(root, PT)).toBe(before);
	});

	test("plural variants come back as a variants array and must have one and other", async () => {
		const root = await makeRepo();
		editJson(root, PT, (o) => void delete o.app_files_count);
		const { ctx, source, area } = app(root);
		const bad = await translateArea(ctx, source, area, "pt-BR", answering({ app_files_count: [{ ...PLURAL("", "")[0], match: { "countPlural=*": "{count} arquivos" } }] }));
		expect(bad.errors.join("\n")).toContain("missing one");
		const ok = await translateArea(ctx, source, area, "pt-BR", answering({ app_files_count: PLURAL("{count} arquivo", "{count} arquivos") }));
		expect(ok.errors).toEqual([]);
		expect(errors(await check(root))).toEqual([]);
	});

	test("a translation may reference a common term the English references", async () => {
		const root = await makeRepo();
		addKey(root, "app_gone", "Moved to {@common_os_trash}", "Movido para {@common_os_trash}", "Toast after a delete");
		editJson(root, PT, (o) => void delete o.app_gone);
		const { ctx, source, area } = app(root);
		const ok = await translateArea(ctx, source, area, "pt-BR", answering({ app_gone: "Movido para {@common_os_trash}" }));
		expect(ok.errors).toEqual([]);
		expect(errors(await check(root))).toEqual([]);
	});

	test("a code fence around the JSON is tolerated", () => {
		expect(parseAnswer('Here you go:\n```json\n{ "a": "b" }\n```\n')).toEqual({ a: "b" });
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
		expect(r.out).toContain("=== app pt-BR (batch 1) ===");
		expect(r.out).toContain("description: Primary toolbar button that saves the open file");
		expect(read(root, PT)).toBe(catalog);
		expect(read(root, LOCK)).toBe(lock);
	});

	test("--area limits the run", async () => {
		const root = await dirtyRepo();
		expect((await run(root, ["--dry-run", "--area", "cli"])).out).toContain("nothing to translate");
		expect((await run(root, ["--dry-run", "--area", "app"])).out).toContain("=== app pt-BR");
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
