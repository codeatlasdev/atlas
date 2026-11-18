// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { compile } from "@inlang/paraglide-js";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { devProjectSettings, paraglideOptions } from "../../../scripts/paraglide.ts";
import { PSEUDO_LOCALE, pseudoEnabled } from "../../../scripts/pseudo-env.ts";
import { locales as compiledLocales } from "#lib/paraglide/runtime.js";
import { LOCALE_NAMES, localeName, parseLocaleState } from "./state.ts";

// en-XA (`bun run i18n:pseudo`) is a layout tool of DEVELOPMENT builds. The shipped app (and the site) list exactly
// the languages of the catalogs; these tests are what keeps the pseudo-locale out of a release.
const repo = join(import.meta.dir, "../../../../..");
const json = (path: string) => JSON.parse(readFileSync(join(repo, path), "utf8"));
const committed = json("apps/desktop/project.inlang/settings.json");

test("only a dev server, a Tauri debug build or an explicit opt-in is a development build", () => {
	const table: [string, string | undefined, Record<string, string>, boolean][] = [
		["vite dev / Atlas Lab", "serve", {}, true],
		["vite build, the release", "build", {}, false],
		["no command, no env (bun run i18n:compile, bun test)", undefined, {}, false],
		["tauri build --debug (desktop:build:app, agent instances)", "build", { TAURI_ENV_DEBUG: "true" }, true],
		["tauri build, the release", "build", { TAURI_ENV_DEBUG: "false" }, false],
		["desktop:build:debug", "build", { ATLAS_PSEUDO_LOCALE: "1" }, true],
		["any other value of the opt-in", "build", { ATLAS_PSEUDO_LOCALE: "yes" }, false],
	];
	for (const [name, command, env, want] of table) expect(pseudoEnabled(command, env), name).toBe(want);
});

test("the release locale list is exactly en + pt-BR: the committed projects, the catalogs and the app's own compile", () => {
	expect(committed.locales).toEqual(["en", "pt-BR"]);
	expect(json("site/project.inlang/settings.json").locales).toEqual(["en", "pt-BR"]);
	expect(Object.keys(json("i18n/locales.json").locales)).toEqual(["en", "pt-BR"]);
	expect(paraglideOptions(false).project).toBe("./project.inlang");
	// What `bun test` compiled is a release compile (no dev command, no TAURI_ENV_DEBUG): no en-XA in its runtime.
	if (!pseudoEnabled()) expect([...compiledLocales]).toEqual(["en", "pt-BR"]);
});

test("a release build refuses en-XA wherever it could come from: the compiled locales, Rust's reply, the picker", () => {
	if (pseudoEnabled()) return;
	expect(
		parseLocaleState({
			locale: PSEUDO_LOCALE,
			preference: PSEUDO_LOCALE,
			source: "env",
			supported: ["en", PSEUDO_LOCALE],
		}),
	).toBeNull();
	expect(
		parseLocaleState({
			locale: "en",
			preference: PSEUDO_LOCALE,
			source: "file",
			supported: ["en", PSEUDO_LOCALE],
		}).supported,
	).toEqual(["en"]);
	// The picker lists `locales` of the compiled runtime and names them: two names, none for the pseudo-locale.
	expect(Object.keys(LOCALE_NAMES)).toEqual(["en", "pt-BR"]);
	expect(compiledLocales.map(localeName)).toEqual(["English", "Português (Brasil)"]);
});

test("the dev project is the committed one plus en-XA; the committed file is never touched", () => {
	const before = JSON.stringify(committed);
	const dev = devProjectSettings(committed);
	expect(dev.locales).toEqual(["en", "pt-BR", "en-XA"]);
	expect(dev.baseLocale).toBe("en");
	expect(dev.modules.every((m) => m.startsWith("/"))).toBe(true);
	expect(dev["plugin.inlang.messageFormat"].pathPattern).toBe("./{locale}.json");
	expect(JSON.stringify(committed)).toBe(before);
	expect(paraglideOptions(true).project).toBe("./.i18n/project.inlang");
	expect(localeName("en-XA")).toBe("Pseudo (en-XA)");
});

// The real Paraglide compiler, on the real dev project settings: en-XA becomes a locale of the runtime and a message
// renders in it (the pseudo-locale's own text), while English and pt-BR are what they were.
test("a dev compile adds en-XA to the runtime; a compile of the committed project does not", async () => {
	const catalogs = {
		en: { save: "Save", files: "Open {count} files" },
		"pt-BR": { save: "Salvar", files: "Abrir {count} arquivos" },
	};
	const compileIn = async (settings: object, files: Record<string, object>) => {
		const dir = mkdtempSync(join(tmpdir(), "atlas-pseudo-"));
		mkdirSync(join(dir, "project.inlang"));
		writeFileSync(join(dir, "project.inlang/settings.json"), JSON.stringify(settings));
		for (const [l, c] of Object.entries(files)) writeFileSync(join(dir, `${l}.json`), JSON.stringify(c));
		await compile({
			project: join(dir, "project.inlang"),
			outdir: join(dir, "out"),
			strategy: ["baseLocale"],
			outputStructure: "message-modules",
		});
		return dir;
	};
	const dev = await compileIn(devProjectSettings(committed), {
		...catalogs,
		[PSEUDO_LOCALE]: { save: "⟦Sávé ~~⟧", files: "⟦Ópéñ {count} fílés ~~~~⟧" },
	});
	const rt = await import(join(dev, "out/runtime.js"));
	const { m } = await import(join(dev, "out/messages.js"));
	expect([...rt.locales]).toEqual(["en", "pt-BR", "en-XA"]);
	expect(rt.isLocale("en-XA")).toBe(true);
	expect(m.save({}, { locale: "en-XA" })).toBe("⟦Sávé ~~⟧");
	expect(m.files({ count: 3 }, { locale: "en-XA" })).toBe("⟦Ópéñ 3 fílés ~~~~⟧");
	expect(m.save({}, { locale: "en" })).toBe("Save");
	expect(m.save({}, { locale: "pt-BR" })).toBe("Salvar");

	const release = await compileIn({ ...devProjectSettings(committed), locales: committed.locales }, catalogs);
	const rrt = await import(join(release, "out/runtime.js"));
	expect([...rrt.locales]).toEqual(["en", "pt-BR"]);
	expect(rrt.isLocale("en-XA")).toBe(false);
});
