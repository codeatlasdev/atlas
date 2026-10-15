// What the flicker sweep films: one entry per surface, each a short script against the real app through the debug
// bridge (`window.__atlas` stores and DOM events, never OS input). `seconds` is the recording length and must outlast
// the script; the script starts 0.6 s after the recorder. Add a surface by adding an entry: nothing else changes.
import type { Fixtures } from "./flicker-sweep-fixtures.ts";

export interface Drive {
	/** Run a JS body (use `return`) in the page and get its JSON result. */
	eval<T = unknown>(js: string): Promise<T>;
	sleep(ms: number): Promise<void>;
	/** DOM click on the first visible match; throws when there is none (a sweep that cannot reach a surface fails). */
	click(selector: string): Promise<void>;
	type(selector: string, text: string, delayMs?: number): Promise<void>;
	/** Resize the main window (logical px). */
	resize(w: number, h: number): Promise<void>;
	fixtures: Fixtures;
}

export interface Surface {
	id: string;
	/** What it exercises, in the report. */
	covers: string;
	seconds: number;
	/**
	 * What must be on screen when `run` ends, as `[what, CSS selector or "js:<expression>"]`. A surface filmed without
	 * its content (a Panel with no service cards) would pass as "no flicker", so the runner checks these and fails the
	 * surface as undriven when one is missing.
	 */
	expects?: Array<[what: string, probe: string]>;
	/** Runs before the recorder rolls (not filmed): put the app in the starting state, like studio's fixtures. */
	setup?(d: Drive): Promise<void>;
	run(d: Drive): Promise<void>;
}

const A = "window.__atlas";
/** The worktree of a project (its main checkout): select it and make sure its layout and Panel exist. */
const select = (project: string) =>
	`const a = ${A}; const p = a.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(project)}); if (!p) throw new Error("no project ${project}");
	 const w = a.ws.doc.worktrees.find(w => w.repoId === p.id && w.isMain); a.ws.view = "worktree"; a.store.selected = w.id; a.ws.select(w.id); a.ws.ensureLayout(w.id); a.ws.ensurePanel(w.id); return w.id`;
/** Open (or focus) a tab of the selected worktree by kind. */
const reveal = (kind: string, label: string) =>
	`const a = ${A}; const w = a.ws.selected; a.ws.reveal(w.id, t => t.kind === ${JSON.stringify(kind)}, { kind: ${JSON.stringify(kind)}, label: ${JSON.stringify(label)} }); return 1`;
const key = (combo: string) => `return window.__atlasCapture.pressKey(${JSON.stringify(combo)})`;
const ids = (project: string) => `${A}.store.projects.find(p => !p.parent && p.name === ${JSON.stringify(project)}).id`;
/** Start the project's whole stack (not awaited: it can outlast the bridge's eval cap). */
const startStack = (project: string) =>
	`window.__TAURI_INTERNALS__.invoke("stack_start", { id: ${ids(project)}, services: [], profiles: [] }).catch(e => console.error("stack_start failed", e)); return 1`;

export const SURFACES: Surface[] = [
	{
		id: "boot",
		covers: "app already booted: idle page, the cursor-less steady state (baseline: any flicker here is the app breathing)",
		seconds: 4,
		run: (d) => d.sleep(3000),
	},
	{
		id: "sidebar-select",
		covers: "sidebar: select project, then back; the space strip; hide and show the sidebar",
		seconds: 14,
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			await d.sleep(800);
			for (const n of ["sweep-docs", "sweep-api", main]) {
				await d.click(`.spaces .space[aria-label="${n}"]`);
				await d.sleep(1200);
			}
			await d.eval(key("ctrl-cmd-s"));
			await d.sleep(1200);
			await d.eval(key("ctrl-cmd-s"));
			await d.sleep(1200);
		},
	},
	{
		id: "space-switch",
		covers: "programmatic space switching: neighbour steps, jump by number, history back/forward",
		seconds: 14,
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.sleep(600);
			for (const js of [`${A}.shell.spaceAt(2)`, `${A}.shell.spaceAt(3)`, `${A}.shell.spaceAt(1)`, `${A}.shell.go(-1)`, `${A}.shell.go(1)`])
				await d.eval(`${js}; return 1`).then(() => d.sleep(1300));
		},
	},
	{
		id: "panel-services",
		covers: "Panel: service cards filling in while the stack starts, a failed (crash-looping) service, metrics updating",
		seconds: 20,
		expects: [["a running service card", "[data-cell] .card.s-running"], ["the failed service card", "[data-cell] .card.failed"]],
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			await d.eval(reveal("services", "Panel"));
			await d.sleep(800);
			await d.eval(startStack(main));
			await d.sleep(14000);
		},
	},
	{
		id: "panel-action-toast",
		covers: "action result toast (success), the output drawer, dismissing it",
		seconds: 14,
		expects: [["the Panel", '[aria-label="Services"]']],
		async setup(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			await d.eval(startStack(main));
			// polled in short evals: the bridge caps one eval at 10 s
			for (let i = 0; i < 40; i++) {
				if (await d.eval<boolean>(`const s = ${A}.store.live[${ids(main)}]?.services ?? []; return s.some(x => x.state === "running")`)) return;
				await d.sleep(500);
			}
			throw new Error("the stack did not come up");
		},
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(reveal("services", "Panel"));
			await d.sleep(600);
			await d.eval(`await window.__TAURI_INTERNALS__.invoke("action_run", { id: ${ids(main)}, target: "x", confirmed: true }); return 1`);
			await d.sleep(4000);
			await d.click('.chip[role="status"] .view');
			await d.sleep(2500);
			await d.eval(key("escape"));
			await d.sleep(1500);
		},
	},
	{
		id: "logs",
		covers: "Logs: streaming lines, search typing, regex toggle, range chips",
		seconds: 16,
		expects: [["the log view", '[role="log"][aria-label="Service logs"]']],
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.eval(reveal("logs", "Logs"));
			await d.sleep(2500);
			await d.type('input[aria-label="Search logs"]', "tick", 90);
			await d.sleep(1200);
			await d.click('button[aria-label="Regular expression"]');
			await d.sleep(1200);
			await d.click('button[aria-label="Regular expression"]');
			await d.sleep(1200);
			await d.eval(`const i = document.querySelector('input[aria-label="Search logs"]'); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(i, ""); i.dispatchEvent(new InputEvent("input", { bubbles: true })); return 1`);
			await d.sleep(2000);
		},
	},
	{
		id: "changes",
		covers: "Changes: open the diff tab, mode select, file tree, scroll the diff",
		seconds: 12,
		expects: [["a changed file in the diff", '.split']],
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.eval(reveal("diff", "Changes"));
			await d.sleep(2500);
			await d.eval(`const s = document.querySelector(".scroll, .diff-scroll, [data-diff-scroll]"); s?.scrollTo({ top: 400, behavior: "smooth" }); return !!s`);
			await d.sleep(2000);
			await d.eval(`const s = document.querySelector(".scroll, .diff-scroll, [data-diff-scroll]"); s?.scrollTo({ top: 0, behavior: "smooth" }); return !!s`);
			await d.sleep(2500);
		},
	},
	{
		id: "settings",
		covers: "Settings: open, every section, the editor tabs, search, close",
		seconds: 24,
		expects: [["the app (settings closed again)", '[aria-label="Settings"]']],
		async run(d) {
			await d.eval(`${A}.updates.show("general"); return 1`);
			await d.sleep(1500);
			for (const t of ["appearance", "editor", "editor/languages", "terminal", "workspaces", "agents", "keyboard", "ssh", "help"]) {
				await d.eval(`${A}.updates.show(${JSON.stringify(t)}); return 1`);
				await d.sleep(1300);
			}
			await d.type('[aria-label="Search settings"]', "font", 80);
			await d.sleep(1200);
			await d.click('[aria-label="Close settings"]');
			await d.sleep(1500);
		},
	},
	{
		id: "dialogs",
		covers: "dialogs open/close: new worktree, find projects, confirm (ask), prompt, what's new drawer",
		seconds: 22,
		expects: [["the app (all dialogs closed)", '[aria-label="Settings"]']],
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			await d.eval(`${A}.worktrees.newDialog.repoId = ${ids(main)}; ${A}.worktrees.newDialog.open = true; return 1`);
			await d.sleep(1800);
			await d.eval(`${A}.worktrees.newDialog.open = false; return 1`);
			await d.sleep(1200);
			await d.eval(`${A}.discovery.open = true; return 1`);
			await d.sleep(1800);
			await d.eval(`${A}.discovery.close(); return 1`);
			await d.sleep(1200);
			await d.eval(`${A}.confirmDialog({ title: "Sweep", message: "Confirm?" }); return 1`);
			await d.sleep(1800);
			await d.eval(key("escape"));
			await d.sleep(1200);
			await d.eval(`${A}.updates.whatsNewOpen = true; return 1`);
			await d.sleep(1800);
			await d.eval(`${A}.updates.whatsNewOpen = false; return 1`);
			await d.sleep(1200);
		},
	},
	{
		id: "menus-tooltips",
		covers: "dropdown menus (New tab, the project menu and a worktree row's menu) and a tooltip: open and close. The project and worktree triggers are hover-only (`display: none` until a pointer hovers the row), so they are opened with a page-side `click()`, which neither hovers nor changes layout",
		seconds: 20,
		expects: [["the New tab trigger", '[aria-label="New tab"]']],
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.sleep(600);
			for (const sel of ['[aria-label="New tab"]', '[aria-label="Project menu"]', '[aria-label="Worktree menu"]']) {
				// the label sits on an inner span and other layers hold copies: take the first match inside the current layer that
				// has a box (hover-only triggers are `display: none`, so fall back to any), and press the kit's trigger the way a
				// pointer does (the kit opens on pointerdown; `click()` alone does not open it)
				const hit = await d.eval<boolean>(
					`const all = [...document.querySelectorAll(${JSON.stringify(sel)})]; const el = all.find((e) => e.getClientRects().length) ?? all[0]; const b = el?.closest('button,[role=button],[aria-haspopup]') ?? el; if (!b) return false; const o = { bubbles: true, cancelable: true, button: 0, pointerType: "mouse", isPrimary: true }; b.dispatchEvent(new PointerEvent("pointerdown", o)); b.dispatchEvent(new PointerEvent("pointerup", o)); b.click(); return true`
				);
				if (!hit) throw new Error(`no element matches ${sel}`);
				await d.sleep(1400);
				const opened = await d.eval<boolean>(`return !!document.querySelector('[role=menu][data-state=open], [data-state=open][role=menu]')`);
				if (!opened) throw new Error(`${sel} did not open a menu`);
				await d.eval(key("escape"));
				await d.sleep(1000);
			}
			// a tooltip: hover is a pointer event the kit's trigger listens to
			await d.eval(`const b = document.querySelector('[aria-label="Settings"]'); b?.dispatchEvent(new PointerEvent("pointerenter", { bubbles: true, pointerType: "mouse" })); b?.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, pointerType: "mouse" })); return !!b`);
			await d.sleep(1500);
			await d.eval(`const b = document.querySelector('[aria-label="Settings"]'); b?.dispatchEvent(new PointerEvent("pointerleave", { bubbles: true, pointerType: "mouse" })); return !!b`);
			await d.sleep(900);
		},
	},
	{
		id: "palette",
		covers: "command palette, quick open and jump: open, type, close",
		seconds: 16,
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.sleep(600);
			for (const combo of ["cmd-shift-p", "cmd-p", "cmd-k"]) {
				await d.eval(key(combo));
				await d.sleep(900);
				await d.type('[role="dialog"] input', "sw", 90).catch(() => undefined);
				await d.sleep(700);
				await d.eval(key("escape"));
				await d.sleep(900);
			}
		},
	},
	{
		id: "toast-error",
		covers: "error toast stack: an uncaught error appears, expands details and is dismissed",
		seconds: 12,
		expects: [["the app (toast dismissed)", '[aria-label="Settings"]']],
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.sleep(600);
			await d.eval(`${A}.throwLater("sweep: uncaught error"); return 1`);
			await d.sleep(2500);
			// "Details" opens a modal over the app: it must be closed (its own Close button) before the toast is dismissed
			await d.click('[role="region"][aria-label="Errors"] button.details');
			await d.sleep(1800);
			await d.click('[role="dialog"][data-state="open"] button:not([aria-label])');
			await d.sleep(1200);
			await d.sleep(2000);
		},
	},
	{
		id: "terminal",
		covers: "terminal: open a native GPU terminal tab, type into it, close it, open again",
		seconds: 18,
		expects: [["the Panel after the terminals closed", '[aria-label="Services"]']],
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			await d.sleep(600);
			for (let i = 0; i < 2; i++) {
				const id = await d.eval<string>(`const a = ${A}; const t = await a.ws.newTerminal(a.ws.selected.id, { argv: [], label: "Terminal" }); if (!t) throw new Error("terminal did not start"); return t.id`);
				await d.sleep(2500);
				await d.eval(`const a = ${A}; const t = a.ws.tab(${JSON.stringify(id)}); await a.host.ptyWrite(t.pty, "ls -la\\r"); return 1`);
				await d.sleep(1500);
				await d.eval(`${A}.ws.closeTab(${JSON.stringify(id)}, true, true); return 1`);
				await d.sleep(1500);
			}
		},
	},
	{
		id: "terminal-resize",
		covers: "window resize with a live terminal (native view follows the slot) and with the Panel",
		seconds: 18,
		async run(d) {
			const { main } = d.fixtures;
			await d.eval(select(main));
			const id = await d.eval<string>(`const a = ${A}; const t = await a.ws.newTerminal(a.ws.selected.id, { argv: [], label: "Terminal" }); return t.id`);
			await d.sleep(2500);
			for (const [w, h] of [[1100, 720], [1000, 680], [1200, 760], [1100, 720]] as const) {
				await d.resize(w, h);
				await d.sleep(1200);
			}
			await d.eval(`${A}.ws.closeTab(${JSON.stringify(id)}, true, true); return 1`);
			await d.sleep(800);
			await d.eval(reveal("services", "Panel"));
			for (const [w, h] of [[1000, 680], [1200, 760]] as const) {
				await d.resize(w, h);
				await d.sleep(1200);
			}
		},
	},
	{
		id: "browser",
		covers: "browser tab: open a native browser view on a local page, then close it",
		seconds: 14,
		async run(d) {
			await d.eval(select(d.fixtures.main));
			await d.sleep(600);
			const id = await d.eval<string>(`const a = ${A}; const t = await a.ws.openTab(a.ws.selected.id, { kind: "browser", label: "Browser", url: "about:blank" }); return t?.id ?? a.ws.focusedGroup(a.ws.selected.id).activeTab`);
			await d.sleep(3500);
			await d.eval(`${A}.ws.closeTab(${JSON.stringify(id)}, true, true); return 1`);
			await d.sleep(2000);
		},
	},
	{
		id: "tabs",
		covers: "tab strip: switch between Panel, Logs, Changes, Config (pane swap), close and reopen a tab",
		seconds: 16,
		async run(d) {
			await d.eval(select(d.fixtures.main));
			for (const [kind, label] of [["logs", "Logs"], ["diff", "Changes"], ["config", "Config"], ["services", "Panel"], ["logs", "Logs"]] as const) {
				await d.eval(reveal(kind, label));
				await d.sleep(1500);
			}
		},
	},
];
