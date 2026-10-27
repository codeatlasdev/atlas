// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const read = (rel) => readFileSync(join(import.meta.dir, rel), "utf8");
const page = read("+page.svelte");
const settings = read("../../lib/components/settings/DockSettings.svelte");
const feed = read("../../lib/stores/dockFeed.svelte.ts");
const agents = read("../../lib/stores/agents.svelte.ts");
const rustMain = read("../../../../../crates/atlas-desktop/src/main.rs");
const rustDock = read("../../../../../crates/atlas-desktop/src/dock.rs");

const invoked = (src) => [...src.matchAll(/invoke(?:<[^>]*>)?\(\s*"([a-z_]+)"/g)].map((m) => m[1]);

// Incident class (term stubs, tray): a command the page calls but `main.rs` never registers fails only at runtime,
// in a window nobody has open. Every command the dock's pages and stores call must be in the handler list.
test("every Tauri command the dock's frontend calls is registered in main.rs", () => {
	const names = new Set([...invoked(page), ...invoked(settings), ...invoked(feed), ...invoked(agents)]);
	for (const n of names) {
		if (!/^(dock_|agent_send)/.test(n)) continue;
		const rust = n.startsWith("agent_") ? `agent::send::${n},` : `dock::${n},`;
		expect(rustMain, `${n} is called by the dock frontend but not registered`).toContain(rust);
	}
	expect([...names].filter((n) => n.startsWith("dock_")).length).toBeGreaterThan(5);
});

test("the events the page listens to are the ones Rust emits, and the reverse", () => {
	const listened = [...page.matchAll(/listen(?:<[^>]*>)?\(\s*"(dock:\/\/[a-z]+)"/g)].map((m) => m[1]).sort();
	const emitted = [...rustDock.matchAll(/"(dock:\/\/[a-z]+)"/g)].map((m) => m[1]);
	expect(listened).toEqual(["dock://attach", "dock://prefs", "dock://snapshot", "dock://summon"]);
	for (const e of listened) expect(emitted, e).toContain(e);
});

// Incident class (tray page): the kit declares its whole token set on a bare `dark` class, which would replace the
// applied theme inside the element.
test("no element of the dock page carries the kit's `dark` class", () => {
	expect(page).not.toMatch(/class:dark|class="[^"]*\bdark\b[^"]*"/);
});

// The dock is a second webview with no workspace: it must not import the stores that need one (they would build a
// second, empty workspace in a window nobody sees and race the real one on disk).
test("the dock page imports no workspace store", () => {
	expect(page).not.toMatch(/stores\/(workspace|projects|agents)\.svelte/);
});

// The hide controls on the pill: a button and the native right-click menu both reach Rust, which persists the choice
// (`dock::toggle`). The pill is a button, so the hide button must be its sibling, never nested (invalid HTML that
// WebKit resolves by dropping the inner button).
test("the pill has a hide button and a context menu, and the button is not inside the pill button", () => {
	expect(page).toContain('invoke("dock_toggle")');
	expect(page).toContain('invoke("dock_menu")');
	const start = page.indexOf('<button class="pill"');
	const pill = page.slice(start, page.indexOf("</button>", start));
	expect(pill).not.toContain('class="hide"');
	expect(page).toContain('aria-label="Hide the dock"');
});

// A capture sent from the quick card while the dock page was not listening must not be lost: Rust queues it, the page
// drains the queue on mount AND whenever `dock://attach` says there is something new, and each drained item becomes
// an `attach` event of the reducer (which dedupes by path).
test("the page drains queued captures on mount and on dock://attach", () => {
	expect(page).toContain('invoke<DockAttachment[]>("dock_take_attachments")');
	expect(page).toMatch(/listen\("dock:\/\/attach",\s*\(\) => void takeAttachments\(\)\)/);
	expect(page).toMatch(/void takeAttachments\(\);\s*\n\s*syncTheme\(\);/);
	expect(page).toContain('dispatch({ type: "attach", path: a.path, kind: a.kind })');
});
