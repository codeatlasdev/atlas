// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";

// Settings > General > Display language: the real row over the real store, with the real kit Select. Only the IPC is
// a fake that answers like `locale_set` does (the real reply shape is pinned by locale.test.ts's Rust fixture).
const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
let refuse: string | null = null;
let mockReply: object | null = null;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, args?: Record<string, unknown>) => {
		calls.push({ cmd, args });
		if (refuse) throw refuse;
		if (mockReply) return mockReply;
		const tag = args?.tag as string | null;
		return { locale: tag ?? "en", preference: tag, source: tag ? "file" : "os", supported: ["en", "pt-BR"] };
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const toasts: string[] = [];
await (await import("../../../../test/uiKit.ts")).useRealKit("Select");
const kit = await import("@codeatlasdev/ui");
kit.toast.error = (m: string) => toasts.push(m);
const { flushSync, mount, tick, unmount } = await import("svelte");
const { appLocale, chooseLocale } = await import("#lib/i18n/locale.svelte.ts");
const { default: DisplayLanguage } = await import("./DisplayLanguage.svelte");

let done: (() => void) | undefined;
afterEach(async () => {
	done?.();
	done = undefined;
	refuse = null;
	mockReply = null;
	// Back to "following the system, in English" (the store is process-wide, like the app's).
	await chooseLocale(null);
	calls.length = 0;
	toasts.length = 0;
	document.body.innerHTML = "";
});

async function render() {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(DisplayLanguage, { target }));
	done = () => unmount(app);
	await tick();
	flushSync();
	return target;
}
async function choose(root: HTMLElement, option: string) {
	root.querySelector(".ca-sel button").click();
	await tick();
	flushSync();
	const item = [...document.querySelectorAll('[role="option"]')].find((o) => o.textContent.trim() === option);
	expect(item, `no option ${option}`).toBeDefined();
	item.click();
	for (let i = 0; i < 5; i++) await tick();
	flushSync();
}

test("the row is named Display language (not the programming Languages tab) and offers Follow system and each language by its own name", async () => {
	const t = await render();
	expect(t.querySelector(".label").textContent).toBe("Display language");
	expect(t.querySelector(".desc").textContent).toBe("The language of Atlas menus, buttons and messages.");
	expect(t.querySelector("h3").textContent).toBe("Language");
	expect(t.querySelector("[data-setting]").dataset.setting).toBe("display-language");
	t.querySelector(".ca-sel button").click();
	await tick();
	flushSync();
	expect([...document.querySelectorAll('[role="option"]')].map((o) => o.textContent.trim())).toEqual([
		"Match system (English)",
		"English",
		"Português (Brasil)",
	]);
});

test("picking Português (Brasil) saves it through Rust and the page switches live, the open row included", async () => {
	const t = await render();
	const loaded = performance.timeOrigin;
	await choose(t, "Português (Brasil)");
	expect(calls).toEqual([{ cmd: "locale_set", args: { tag: "pt-BR" } }]);
	expect(appLocale()).toBe("pt-BR");
	expect(t.querySelector(".label").textContent).toBe("Idioma de exibição");
	expect(t.querySelector("h3").textContent).toBe("Idioma");
	expect(t.querySelector(".desc").textContent).toBe("Idioma dos menus, botões e mensagens do Atlas.");
	expect(performance.timeOrigin).toBe(loaded);
	// With a saved choice the system's language is unknown to the page: the bare words, now in the new language. The
	// endonyms never change.
	t.querySelector(".ca-sel button").click();
	await tick();
	flushSync();
	expect([...document.querySelectorAll('[role="option"]')].map((o) => o.textContent.trim())).toEqual([
		"Igual ao sistema",
		"English",
		"Português (Brasil)",
	]);
});

test("Match system sends no tag, so Rust removes the saved choice, and names the language it resolves to", async () => {
	const t = await render();
	await choose(t, "Português (Brasil)");
	calls.length = 0;
	await choose(t, "Igual ao sistema");
	expect(calls).toEqual([{ cmd: "locale_set", args: { tag: null } }]);
	expect(appLocale()).toBe("en");
	t.querySelector(".ca-sel button").click();
	await tick();
	flushSync();
	expect(document.querySelector('[role="option"]').textContent.trim()).toBe("Match system (English)");
});

test("following the system in Portuguese reads 'Igual ao sistema (Português (Brasil))' with the resolved language", async () => {
	mockReply = { locale: "pt-BR", preference: null, source: "os", supported: ["en", "pt-BR"] };
	await chooseLocale(null);
	const t = await render();
	t.querySelector(".ca-sel button").click();
	await tick();
	flushSync();
	expect(document.querySelector('[role="option"]').textContent.trim()).toBe(
		"Igual ao sistema (Português (Brasil))",
	);
});

test("a refused save is a toast in the current language and the UI stays as it was", async () => {
	refuse = "disk full";
	const t = await render();
	await choose(t, "Português (Brasil)");
	expect(toasts).toEqual(["Could not change the language: disk full"]);
	expect(appLocale()).toBe("en");
	expect(t.querySelector(".label").textContent).toBe("Display language");
});
