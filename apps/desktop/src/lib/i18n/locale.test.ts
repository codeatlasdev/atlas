// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, beforeEach, expect, mock, test } from "bun:test";
import localeStateFixture from "../__fixtures__/locale-state.json";

// The Rust side of the contract is `crates/atlas-desktop/src/cmds/locale.rs`: its test writes the fixture from the REAL
// serialized `LocaleState`, and these tests feed it to the parser the app uses on the same reply. Only the IPC is a
// fake that answers like Rust does.
type Args = Record<string, unknown>;
const calls: { cmd: string; args?: Args }[] = [];
let reply: unknown = localeStateFixture;
let listener: ((e: { payload: unknown }) => void) | undefined;
// `mock.module` is process-wide and live: a module another file imported earlier (a store with a poll, a timer) calls
// THIS `invoke` for the rest of the run. So it answers only the commands under test, and everything else gets the
// harmless `null` the other suites' mocks give; a refusal must never outlive its test (`afterEach` resets it).
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args?: Args) => {
		if (cmd !== "locale_get" && cmd !== "locale_set") return null;
		calls.push({ cmd, args });
		if (reply instanceof Error) throw reply;
		return reply;
	},
}));
mock.module("@tauri-apps/api/event", () => ({
	listen: async (_name: string, cb: typeof listener) => {
		listener = cb;
		return () => {};
	},
}));
const { flushSync, mount, unmount } = await import("svelte");
const { LOCALE_NAMES, parseLocaleState } = await import("./state.ts");
const {
	appLocale,
	bootLocale,
	chooseLocale,
	localePreference,
	localeSource,
	setAppLocale,
	syncDocumentLocale,
} = await import("./locale.svelte.ts");
const { default: Harness } = await import("../../../test/MessageHarness.svelte");
const { inLocale } = await import("../../../test/locale.ts");

beforeEach(() => {
	calls.length = 0;
	reply = localeStateFixture;
	listener = undefined;
	// A plain browser has no Tauri; the tests that need it ask for it.
	delete window.__TAURI_INTERNALS__;
});
afterEach(() => {
	reply = localeStateFixture;
	setAppLocale("en");
	document.body.innerHTML = "";
});

test("the real locale_get reply parses: the language, the saved choice, what decided it, what exists", () => {
	expect(parseLocaleState(localeStateFixture)).toEqual({
		locale: "pt-BR",
		preference: "pt-BR",
		source: "file",
		supported: ["en", "pt-BR"],
	});
});

test("a reply that is not a locale state is rejected, and unknown parts degrade instead of throwing", () => {
	expect(parseLocaleState(null)).toBeNull();
	expect(parseLocaleState([])).toBeNull();
	expect(parseLocaleState({ locale: "fr" })).toBeNull();
	expect(
		parseLocaleState({ locale: "en", preference: "fr", source: "magic", supported: ["en", "xx"] }),
	).toEqual({
		locale: "en",
		preference: null,
		source: "default",
		supported: ["en"],
	});
});

test("every language is shown under its own name, never translated", () => {
	expect(LOCALE_NAMES).toEqual({ en: "English", "pt-BR": "Português (Brasil)" });
});

test("a switch re-renders what is on screen in place, with no reload and no new node", () => {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Harness, { target, props: { count: 0, at: 0, now: 5 * 60_000 } }));
	const label = target.querySelector("#label");
	expect(label.textContent).toBe("Display language");
	expect(target.querySelector("#count").textContent).toBe("Up to date · no running terminals");
	expect(target.querySelector("#ago").textContent).toBe("5 min ago");
	const loaded = performance.timeOrigin;

	setAppLocale("pt-BR");
	flushSync();
	expect(target.querySelector("#label")).toBe(label);
	expect(label.textContent).toBe("Idioma de exibição");
	expect(target.querySelector("#count").textContent).toBe("Atualizado · sem terminais abertos");
	expect(target.querySelector("#ago").textContent).toBe("há 5 min");
	expect(performance.timeOrigin).toBe(loaded);

	setAppLocale("en");
	flushSync();
	expect(label.textContent).toBe("Display language");
	unmount(app);
});

test("the very first render of a message does not throw state_unsafe_mutation (the guard in the strategy)", async () => {
	// Fresh process: the trap happens once per process, at the first `getLocale()`. Without the changed-only guard in
	// `setLocale` of locale.svelte.ts, Paraglide's initial `setLocale(resolved)` writes the $state inside this render.
	const p = Bun.spawnSync(
		["bun", "--conditions", "browser", "--preload", "./test/setup.ts", "./test/firstRenderProbe.ts"],
		{ cwd: new URL("../../..", import.meta.url).pathname, env: { ...process.env, TZ: "UTC" } },
	);
	const lines = p.stdout.toString().trim().split("\n");
	const out = JSON.parse(lines[lines.length - 1]);
	expect(out.error, p.stderr.toString()).toBeNull();
	expect(out.first).toBe("Display language");
	expect(out.switched).toBe("Idioma de exibição | Atualizado · 2 terminais abertos | há 5 min");
	expect(out.sameNode).toBe(true);
});

test("<html lang> and dir follow the language", () => {
	const stop = syncDocumentLocale();
	flushSync();
	expect(document.documentElement.lang).toBe("en");
	expect(document.documentElement.dir).toBe("ltr");
	setAppLocale("pt-BR");
	flushSync();
	expect(document.documentElement.lang).toBe("pt-BR");
	expect(document.documentElement.dir).toBe("ltr");
	expect(syncDocumentLocale()).toBe(stop);
});

test("inLocale renders in another language and puts the previous one back", async () => {
	await inLocale("pt-BR", () => expect(appLocale()).toBe("pt-BR"));
	expect(appLocale()).toBe("en");
});

test("boot reads the language from Rust before anything renders", async () => {
	window.__TAURI_INTERNALS__ = {};
	await bootLocale();
	expect(calls).toEqual([{ cmd: "locale_get", args: undefined }]);
	expect(appLocale()).toBe("pt-BR");
	expect(localePreference()).toBe("pt-BR");
	expect(localeSource()).toBe("file");
});

test("boot in a plain browser keeps the base language and asks nobody", async () => {
	await bootLocale();
	expect(calls).toEqual([]);
	expect(appLocale()).toBe("en");
});

test("boot with Tauri but a broken command logs it and still opens in English", async () => {
	window.__TAURI_INTERNALS__ = {};
	reply = { locale: "klingon" };
	const errors = [];
	const spy = mock((...a) => errors.push(a.join(" ")));
	const real = console.error;
	console.error = spy;
	try {
		await bootLocale();
	} finally {
		console.error = real;
	}
	expect(errors.join()).toContain("display language unavailable");
	expect(appLocale()).toBe("en");
});

test("another window's switch arrives as an event", async () => {
	window.__TAURI_INTERNALS__ = {};
	await bootLocale();
	listener({ payload: { locale: "en", preference: "en", source: "file", supported: ["en", "pt-BR"] } });
	expect(appLocale()).toBe("en");
	expect(localePreference()).toBe("en");
});

test("choosing saves through Rust and renders the language Rust answers with", async () => {
	reply = { locale: "en", preference: null, source: "os", supported: ["en", "pt-BR"] };
	setAppLocale("pt-BR");
	await chooseLocale(null);
	expect(calls).toEqual([{ cmd: "locale_set", args: { tag: null } }]);
	expect(appLocale()).toBe("en");
	expect(localePreference()).toBeNull();
	expect(localeSource()).toBe("os");
});

test("a choice Rust refuses leaves the language as it was and rejects with the reason", async () => {
	reply = new Error('unsupported language "fr"; supported: en, pt-BR');
	await expect(chooseLocale("pt-BR")).rejects.toThrow("unsupported language");
	expect(appLocale()).toBe("en");
});
