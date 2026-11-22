// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { createRequire } from "node:module";
import { afterAll, afterEach, beforeEach, expect, mock, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";

// Incident #503 (1.5.0-alpha.26): the right panel showed "No sessions yet." and nothing else. Real component, real kit,
// the real stores; only the Tauri bridge is stubbed (a global, not a module mock, which would outlive this file).
const calls: { cmd: string; args: unknown }[] = [];
let answers: Record<string, unknown> = {};
const invoke = async (cmd: string, args: unknown) => {
	calls.push({ cmd, args });
	const a = answers[cmd];
	return typeof a === "function" ? a(args) : (a ?? null);
};
// Other suites mock `@tauri-apps/api/core` process-wide (their mock is still in place when this file runs), so a
// `__TAURI_INTERNALS__` stub alone would never be reached: the bridge is mocked here too, with the same recorder.
// Every other export stays the real one (by file, which a module mock of the specifier cannot replace): the event and
// window APIs link against `transformCallback` and friends.
const realCore = await import(createRequire(import.meta.url).resolve("@tauri-apps/api/core"));
mock.module("@tauri-apps/api/core", () => ({ ...realCore, invoke }));
window.__TAURI_INTERNALS__ = { invoke, transformCallback: () => 0 };
mock.module("@tauri-apps/api/window", () => ({
	getCurrentWindow: () => ({ onFocusChanged: async () => () => {} }),
}));
await (
	await import("../../../test/uiKit.ts")
).useRealKit("Select", "Skeleton", "EmptyState", "Tooltip", "Spinner");
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: RightSidebar } = await import("./RightSidebar.svelte");
const { ws } = await import("#lib/stores/workspace.svelte.ts");
const { agents } = await import("#lib/stores/agents.svelte.ts");
const { store } = await import("#lib/stores/projects.svelte.ts");
const { docks, peek } = await import("#lib/stores/docks.svelte.ts");
const { shortcutFor } = await import("#lib/keymap.svelte.ts");

afterAll(() => {
	delete window.__TAURI_INTERNALS__;
});

const WT = {
	id: "w1",
	repoId: "w1",
	path: "/repo",
	branch: "main",
	baseRef: "main",
	name: "main",
	isMain: true,
	archived: false,
	pinned: false,
	unread: false,
	portOffset: 0,
	createdAt: 0,
	lastActivityAt: 0,
};
const svc = (name: string, port: number | null) => ({
	name,
	port,
	enabled: true,
	kind: "api",
	open_key: null,
	target: null,
	command: null,
	dir: null,
	depends_on: [],
});

const realSync = ws.syncPtys;
let target: HTMLElement | undefined;
let app: ReturnType<typeof mount> | undefined;
function render() {
	target = document.createElement("div");
	document.body.append(target);
	app = flushSync(() => mount(RightSidebar, { target }));
	return target;
}
/** Let the invokes and the effects they feed settle. */
const settle = async () => {
	for (let i = 0; i < 6; i++) await tick();
	await new Promise((r) => setTimeout(r, 0));
	await tick();
};
const text = () => target!.textContent!.replace(/\s+/g, " ");
const button = (label: RegExp) =>
	[...target!.querySelectorAll("button")].find((b) =>
		label.test(b.getAttribute("aria-label") ?? b.textContent ?? ""),
	);

beforeEach(() => {
	calls.length = 0;
	answers = { agent_sessions: [], host_pty_list: [], listening_ports: [] };
	ws.doc.worktrees = [WT];
	ws.doc.selected = "w1";
	ws.doc.tabs = [];
	ws.doc.groups = [];
	ws.ptys = {};
	agents.status = {};
	store.configs = {};
	store.live = {};
	docks.right = false;
	peek.right = true; // the panel is on screen
});
afterEach(() => {
	ws.syncPtys = realSync;
	if (app) unmount(app);
	target?.remove();
	app = target = undefined;
});

const session = (over = {}) => ({
	agent: "omp",
	id: "s1",
	resume: "/sessions/s1.jsonl",
	file: "/sessions/s1.jsonl",
	cwd: "/repo",
	title: "Fix the flaky panel",
	first_prompt: "the right panel opens only sometimes",
	mtime: Math.floor(Date.now() / 1000) - 5 * 60,
	size: 10,
	...over,
});

test("agents tab, nothing yet: says what the panel is for and how to start", async () => {
	render();
	await settle();
	expect(text()).toContain("Sessions of agents you run in this worktree");
	const start = button(/New agent session/);
	expect(start).toBeTruthy();
	// The keymap's own label of the command (a user remap shows too); the platform spelling is edgePeek.test.ts's.
	expect(shortcutFor("atlas::NewAgentSession")).toBeTruthy();
	expect(start.textContent).toContain(shortcutFor("atlas::NewAgentSession"));
	expect(text()).not.toContain("No sessions yet.");
});

test("a search with no match says so, and does not tell the user to start an agent", async () => {
	render();
	await settle();
	const input = target.querySelector("input");
	input.value = "zzz";
	input.dispatchEvent(new Event("input", { bubbles: true }));
	input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
	await settle();
	expect(text()).toContain(m.sidebar_right_no_match({ query: "zzz" }));
	expect(button(/New agent session/)).toBeUndefined();
});

test("agents tab, populated: the agents running here with their state and last words, then the history with ages", async () => {
	answers.agent_sessions = [session()];
	ws.doc.tabs = [
		{
			id: "t1",
			worktreeId: "w1",
			groupId: "g",
			kind: "terminal",
			label: "omp",
			pty: "p1",
			agent: "omp",
			createdAt: 0,
		},
		{
			id: "t2",
			worktreeId: "w1",
			groupId: "g",
			kind: "terminal",
			label: "claude",
			pty: "p2",
			agent: "claude",
			createdAt: 0,
		},
		{
			id: "t3",
			worktreeId: "other",
			groupId: "g",
			kind: "terminal",
			label: "omp",
			pty: "p3",
			agent: "omp",
			createdAt: 0,
		},
	];
	ws.doc.groups = [{ id: "g", worktreeId: "w1", activeTab: null, order: ["t1", "t2"] }];
	const st = (state, last_message) => ({
		state,
		agent: "x",
		session_id: null,
		session_file: null,
		model: null,
		prompt: null,
		last_message,
		updated_at: Date.now(),
		state_started_at: Date.now() - 120_000,
		worktree_id: "w1",
		tab_id: null,
	});
	agents.status = {
		t1: st("working", "Reading the edge peek code"),
		t2: st("waiting", "Apply the patch?"),
		t3: st("working", "elsewhere"),
	};
	render();
	await settle();
	const rows = [...target.querySelectorAll("[data-live-agent]")];
	expect(rows.map((r) => r.getAttribute("data-live-agent"))).toEqual(["t2", "t1"]); // needs you first; t3 is another worktree
	expect(rows[0].textContent).toContain("Needs you");
	expect(rows[0].textContent).toContain("Apply the patch?");
	expect(rows[1].textContent).toContain("Working");
	expect(rows[1].textContent).toContain("Reading the edge peek code");
	expect(rows[1].textContent).toContain("2 min ago"); // in that state for two minutes
	expect(text()).toContain("Fix the flaky panel");
	expect(text()).toContain("5 min ago");
	expect(text()).not.toContain("Sessions of agents you run"); // not empty
	rows[1].click();
	expect(ws.group("g").activeTab).toBe("t1"); // a click brings that terminal forward
});

test("ports tab, nothing listening: explains where ports come from", async () => {
	render();
	await settle();
	button(/^Ports$/).click();
	await settle();
	expect(text()).toContain("dev server");
	expect(text()).toContain("terminal");
	expect(text()).not.toContain("No listening ports in this worktree's terminals.");
});

// Two bugs of the old tab: the pids were read from the pty list BEFORE `syncPtys` refreshed it (a terminal opened since
// the last sync never showed its server), and a port had no service name and no way out but the in-app tab.
test("ports tab, populated: a fresh terminal's ports, named after the service that declares them, and an open button", async () => {
	// What a fresh `syncPtys` learns: a terminal that was not in `ws.ptys` when the panel was asked for its ports.
	// (`host.ts` is mocked process-wide by other suites, so the sync itself is replaced here, on the real store.)
	let syncs = 0;
	ws.syncPtys = async () => {
		syncs++;
		ws.ptys = {
			p1: {
				pty: "p1",
				cwd: "/repo",
				argv: [],
				scope: { worktree_id: "w1" },
				title: "",
				created_at: 0,
				exited: null,
				pid: 4242,
			},
		};
	};
	answers.listening_ports = [
		{ port: 5173, pid: 4242, command: "node" },
		{ port: 3000, pid: 4242, command: "bun" },
	];
	store.configs = {
		w1: {
			name: "app",
			services: [svc("api", 3000), svc("web", 8080)],
			actions: [],
			setup: {},
			detected: null,
		},
	};
	store.live = {
		w1: {
			services: [
				{ name: "api", state: "running" },
				{ name: "web", state: "running" },
			],
		},
	};
	render();
	await settle();
	button(/^Ports$/).click();
	await settle();
	// Once: reading the terminals before the sync, inside the effect, made the effect depend on what the sync writes, so
	// an open Ports tab re-listed terminals and ports in a loop.
	await settle();
	expect(syncs).toBe(1);
	const ports = calls.filter((c) => c.cmd === "listening_ports");
	expect(ports.at(-1).args).toEqual({ pids: [4242] }); // read after the sync, not from the stale (empty) map
	const rows = [...target.querySelectorAll("[data-port]")].map((r) => [
		r.getAttribute("data-port"),
		r.textContent.replace(/\s+/g, " ").trim(),
	]);
	expect(rows.map((r) => r[0])).toEqual(["3000", "5173", "8080"]);
	expect(rows[0][1]).toContain("api");
	expect(rows[0][1]).toContain("bun (4242)");
	expect(rows[1][1]).toContain("node (4242)");
	expect(rows[2][1]).toContain("web"); // started by `atlas dev`, in no terminal: still listed
	button(/Open :5173 in the system browser/).click();
	await settle();
	expect(calls.filter((c) => c.cmd === "open_url").at(-1).args).toEqual({ url: "http://localhost:5173" });
});

test("the panel reads fresh data each time it comes on screen", async () => {
	peek.right = false;
	render();
	await settle();
	const before = calls.filter((c) => c.cmd === "agent_sessions").length;
	peek.right = true;
	await settle();
	expect(calls.filter((c) => c.cmd === "agent_sessions").length).toBeGreaterThan(before);
});
