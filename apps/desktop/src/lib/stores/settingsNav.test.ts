// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, expect, mock, test } from "bun:test";

// What `updates.show(...)`, a sidebar click and a search result do to the Settings dialog's state. The registry
// resolves ids (registry.test.ts); this pins what the store does with the answer: the section, the tab it keeps
// per section, the row to reveal and the search box.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));

// Dynamic on purpose: static imports are hoisted above `mock.module`, so the store would load the real Tauri API.
const { updates: u } = await import("./updates.svelte.ts");
const { openSection } = await import("../settings/index.ts");
u.check = async () => {};

beforeEach(() => {
	u.open = false;
	u.query = "";
	u.reveal = null;
	u.select({ section: "general" });
});

test("show(id) opens the dialog on the section, tab and row the id names", () => {
	u.show("shell");
	expect(u.open).toBe(true);
	expect([u.section, u.tab, u.reveal]).toEqual(["integrations", "shell", "shell"]);
});

test("a section id has nothing to reveal", () => {
	u.show("editor/languages");
	expect([u.section, u.tab, u.reveal]).toEqual(["editor", "languages", null]);
});

test("show() with no id reopens where Settings was, and clears the search box", () => {
	u.show("terminal");
	u.open = false;
	u.query = "font";
	u.show();
	expect(u.section).toBe("terminal");
	expect(u.query).toBe("");
	expect(u.open).toBe(true);
});

test("an id of an older version still lands on its content", () => {
	u.show("appearance/terminal");
	expect(u.section).toBe("terminal");
	u.show("ssh");
	expect([u.section, u.tab]).toEqual(["integrations", "ssh"]);
});

test("a sidebar click opens the section on its first tab, not on the last tab used elsewhere", () => {
	u.show("editor/languages");
	u.select(openSection("integrations"));
	expect([u.section, u.tab]).toEqual(["integrations", "shell"]);
	u.select(openSection("editor"));
	expect([u.section, u.tab]).toEqual(["editor", "editor"]);
});

test("a search result goes to its tab and names its row", () => {
	u.select({ section: "editor", tab: "languages", item: "lsp-custom" });
	expect([u.section, u.tab, u.reveal]).toEqual(["editor", "languages", "lsp-custom"]);
});

test("every navigation clears the search box", () => {
	u.query = "x";
	u.select({ section: "git" });
	expect(u.query).toBe("");
});

test("opening on General checks for updates; any other section does not", () => {
	let checks = 0;
	u.check = async () => void checks++;
	u.show("git");
	expect(checks).toBe(0);
	u.show("updates");
	expect(checks).toBe(1);
});
