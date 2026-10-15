import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Tab, Worktree } from "#lib/stores/workspace.svelte.ts";
import { openFile, openDiff } from "#lib/stores/files.svelte.ts";
import { openPreview, toUrl } from "#lib/stores/browser.svelte.ts";
import { nativeRect } from "#lib/zoom.ts";
import { errText } from "#lib/api.ts";
import { realEnv } from "#lib/capture/env.ts";
import { runScript } from "#lib/capture/runner.ts";
import { parseScript } from "#lib/capture/script.ts";

// These getters resolve through window.__atlasCtlStores (set by shell/session.svelte.ts)
// so ctl.ts works regardless of which module instance calls it.
interface CtlStores {
	ws: typeof import("#lib/stores/workspace.svelte.ts").ws;
	store: typeof import("#lib/stores/projects.svelte.ts").store;
	agents: typeof import("#lib/stores/agents.svelte.ts").agents;
	worktrees: typeof import("#lib/stores/worktrees.svelte.ts").worktrees;
	api: typeof import("#lib/api.ts").api;
	host: typeof import("#lib/host.ts").host;
}
function live(): CtlStores {
	const s = (window as Window & { __atlasCtlStores?: CtlStores }).__atlasCtlStores;
	if (!s) throw new Error("Atlas Desktop stores not ready (window not mounted yet)");
	return s;
}

type Args = Record<string, unknown>;

const str = (a: Args, k: string) => (typeof a[k] === "string" ? (a[k] as string) : undefined);
const IDLE_STATES: Record<string, true> = { idle: true, done: true, waiting: true, blocked: true };
const IDLE_QUIET_MS = 2_000;

function findWorktree(ref: string | undefined): Worktree {
	if (!ref) throw new Error("worktree required (id, name or path)");
	const all = live().ws.doc.worktrees;
	const w =
		all.find((x) => x.id === ref) ??
		all.find((x) => x.name === ref || x.branch === ref) ??
		all
			.filter((x) => ref === x.path || ref.startsWith(`${x.path}/`))
			.sort((a, b) => b.path.length - a.path.length)[0] ??
		(ref.length >= 4 ? all.find((x) => x.id.startsWith(ref)) : undefined);
	if (!w) throw new Error(`no worktree ${ref}`);
	return w;
}

function wtJson(w: Worktree) {
	const p = live().store.projects.find((x) => x.id === w.id);
	return {
		id: w.id,
		name: w.isMain ? "main" : w.name,
		repo: live().store.projects.find((x) => x.id === w.repoId)?.name ?? w.repoId,
		path: w.path,
		branch: p?.branch ?? w.branch,
		main: w.isMain,
		port_offset: w.portOffset,
		unread: w.unread,
		pr: w.linkedPr ?? null,
		running: p?.running ?? false,
		comment: w.comment ?? null,
	};
}

function termJson(t: Tab) {
	const st = live().agents.status[t.id];
	return {
		pty: t.pty,
		tab: t.id,
		worktree: t.worktreeId,
		label: (t.pty && live().ws.titles[t.pty]) || t.label,
		agent: t.agent ?? null,
		state: t.agent ? (st?.state ?? "idle") : null,
		exited: t.pty && t.pty in live().ws.exited ? { code: live().ws.exited[t.pty] } : null,
	};
}

function terminalTab(pty: string | undefined): Tab {
	const t = live().ws.doc.tabs.find((x) => x.pty === pty || x.id === pty);
	if (!t?.pty) throw new Error(`no terminal ${pty}`);
	return t;
}

function browserTab(a: Args): Tab {
	const id = str(a, "tab");
	const tabs = live().ws.doc.tabs.filter((t) => t.kind === "browser");
	const t = id
		? tabs.find((x) => x.id === id || x.id.startsWith(id))
		: (tabs.find((x) => live().ws.group(x.groupId)?.activeTab === x.id) ?? tabs[0]);
	if (!t) throw new Error("no browser tab; open one with `atlas browser goto <url>`");
	return t;
}

/** Show a worktree in the app (the same three steps as the sidebar click). */
function focusWorktree(id: string) {
	live().ws.view = "worktree";
	live().store.selected = id;
	live().ws.select(id);
}

/** Run a `window.__atlasGrab` call in a browser tab's page. */
function grab(tab: Tab, call: string) {
	return invoke("browser_eval", { tab: tab.id, expr: `window.__atlasGrab.${call}` });
}

async function waitFor<T>(check: () => T | undefined, timeoutS: number): Promise<T> {
	const end = Date.now() + timeoutS * 1000;
	for (;;) {
		const v = check();
		if (v !== undefined) return v;
		if (Date.now() > end) throw new Error("timeout");
		await new Promise((r) => setTimeout(r, 250));
	}
}

const handlers: Record<string, (a: Args) => Promise<unknown>> = {
	async status() {
		return {
			worktrees: live().ws.doc.worktrees.length,
			terminals: live().ws.doc.tabs.filter((t) => t.pty).length,
			selected: live().ws.doc.selected,
			agents: Object.fromEntries(
				live()
					.ws.doc.tabs.filter((t) => t.agent)
					.map((t) => [t.id, live().agents.status[t.id]?.state ?? "idle"]),
			),
		};
	},

	async worktree_list(a) {
		const repo = str(a, "repo");
		return live()
			.ws.doc.worktrees.filter((w) => !w.archived || a.archived)
			.filter(
				(w) =>
					!repo || w.repoId === repo || live().store.projects.find((p) => p.id === w.repoId)?.name === repo,
			)
			.map(wtJson);
	},

	async worktree_current(a) {
		return wtJson(findWorktree(str(a, "cwd")));
	},

	async worktree_create(a) {
		const ref = str(a, "repo");
		if (!ref) throw new Error("--repo required");
		let repo = live().store.projects.find(
			(p) => !p.parent && (p.id === ref || p.name === ref || p.root === ref),
		);
		if (!repo) {
			const w = live().ws.doc.worktrees.find((x) => ref === x.path || ref.startsWith(`${x.path}/`));
			repo = live().store.projects.find((p) => p.id === (w?.repoId ?? ""));
		}
		if (!repo) {
			const added = await live().api.add(ref);
			await live().store.refresh();
			repo = live().store.projects.find((p) => p.id === added.id);
		}
		if (!repo) throw new Error(`no project at ${ref}`);
		const id = await live().worktrees.create({
			repoId: repo.id,
			name: str(a, "name"),
			base: str(a, "base"),
			pr: typeof a.pr === "number" ? a.pr : undefined,
			runSetup: a.setup !== "skip",
			agent: str(a, "agent") ?? "blank",
			prompt: str(a, "prompt"),
		});
		if (!id) throw new Error("worktree creation failed (see the app)");
		const w = live().ws.worktree(id)!;
		return {
			...wtJson(w),
			terminals: live()
				.ws.tabsOf(id)
				.filter((t) => t.pty)
				.map(termJson),
		};
	},

	async worktree_remove(a) {
		const w = findWorktree(str(a, "id"));
		if (w.isMain) throw new Error("the main checkout cannot be removed");
		const r = await live().api.worktreeRemove(w.id, a.force === true);
		live().ws.removeWorktree(w.id);
		await live().store.refresh();
		return r;
	},

	async worktree_set(a) {
		const w = findWorktree(str(a, "id"));
		if (typeof a.unread === "boolean") w.unread = a.unread;
		if (typeof a.name === "string") w.name = a.name;
		if (typeof a.comment === "string") w.comment = a.comment;
		return wtJson(w);
	},

	async terminal_list(a) {
		const ref = str(a, "worktree");
		const w = ref ? findWorktree(ref) : undefined;
		return live()
			.ws.doc.tabs.filter((t) => t.pty && (!w || t.worktreeId === w.id))
			.map(termJson);
	},

	async terminal_create(a) {
		const w = findWorktree(str(a, "worktree"));
		const agent = str(a, "agent");
		const t = agent
			? await live().agents.launch(w.id, agent, { prompt: str(a, "prompt") })
			: await live().ws.newTerminal(w.id, {
					argv: Array.isArray(a.argv) ? (a.argv as string[]) : [],
				});
		if (!t) throw new Error("could not start the terminal");
		return termJson(t);
	},

	async terminal_send(a) {
		const t = terminalTab(str(a, "pty"));
		const text = str(a, "text") ?? "";
		if (t.agent && a.enter) await live().agents.send(t.id, text);
		else await live().host.ptyWrite(t.pty!, a.enter ? `${text}\r` : text);
		return {};
	},

	async terminal_kill(a) {
		const t = terminalTab(str(a, "pty"));
		live().ws.closeTab(t.id);
		return {};
	},

	async terminal_read(a) {
		const t = terminalTab(str(a, "pty"));
		return {
			text: await live().host.ptyRead(t.pty!, typeof a.lines === "number" ? a.lines : 200),
		};
	},

	async terminal_wait(a) {
		const t = terminalTab(str(a, "pty"));
		const timeout = typeof a.timeout_s === "number" ? a.timeout_s : 600;
		if (a.for === "exit") {
			const code = await waitFor(
				() => (t.pty! in live().ws.exited ? { code: live().ws.exited[t.pty!] } : undefined),
				timeout,
			);
			return { state: "exit", ...code };
		}
		if (t.agent) {
			const since = Date.now();
			await new Promise((r) => setTimeout(r, 1000));
			const st = await waitFor(() => {
				const s = live().agents.status[t.id];
				if (!s) return Date.now() - since > IDLE_QUIET_MS ? "idle" : undefined;
				return IDLE_STATES[s.state] ? s.state : undefined;
			}, timeout);
			return { state: st, last_message: live().agents.status[t.id]?.last_message ?? null };
		}
		await live().host.ptyRead(t.pty!, 1);
		let last = await live().host.ptyRead(t.pty!, 50);
		let quietSince = Date.now();
		await waitFor(() => (Date.now() - quietSince >= IDLE_QUIET_MS ? true : undefined), timeout).catch(
			() => {},
		);
		for (;;) {
			const now = await live().host.ptyRead(t.pty!, 50);
			if (now === last) break;
			last = now;
			quietSince = Date.now();
			await new Promise((r) => setTimeout(r, IDLE_QUIET_MS));
		}
		return { state: "idle" };
	},

	async file_open(a) {
		const w = findWorktree(str(a, "worktree"));
		const path = str(a, "path") ?? "";
		const rel = path.startsWith(`${w.path}/`) ? path.slice(w.path.length + 1) : path;
		if (a.focus !== false) focusWorktree(w.id);
		openFile(w.id, rel);
		return { worktree: w.id, path: rel };
	},

	async diff_open(a) {
		const w = findWorktree(str(a, "worktree"));
		focusWorktree(w.id);
		openDiff(w.id);
		return { worktree: w.id };
	},

	async browser_goto(a) {
		const url = toUrl(str(a, "url") ?? "");
		const existing = str(a, "tab") ? browserTab(a) : undefined;
		if (existing) {
			await invoke("browser_navigate", { tab: existing.id, url });
			live().ws.activate(existing.id);
			return { tab: existing.id, url };
		}
		const w = findWorktree(str(a, "worktree") ?? live().ws.doc.selected ?? undefined);
		focusWorktree(w.id);
		await openPreview(w.id, url);
		const t = live()
			.ws.tabsOf(w.id)
			.find((x) => x.kind === "browser" && x.url === url)!;
		await new Promise((r) => setTimeout(r, 1500));
		return { tab: t.id, url };
	},

	async browser_snapshot(a) {
		const t = browserTab(a);
		live().ws.activate(t.id);
		return grab(t, "snapshot()");
	},

	async browser_click(a) {
		const t = browserTab(a);
		return grab(t, `clickRef(${JSON.stringify(str(a, "ref"))})`);
	},

	async browser_fill(a) {
		const t = browserTab(a);
		return grab(t, `fillRef(${JSON.stringify(str(a, "ref"))}, ${JSON.stringify(str(a, "text") ?? "")})`);
	},

	/** Scripted recording: `{script, out, shots_dir?}` -> `{ok, error?, timeline}` (see lib/capture). */
	async capture_run(a) {
		const parsed = parseScript(a.script);
		if (!parsed.ok)
			throw new Error(
				`invalid script:\n${parsed.errors.map((e) => `  ${e.path || "(script)"}: ${e.message}`).join("\n")}`,
			);
		const out = str(a, "out");
		if (!out) throw new Error("out is required (absolute path of the video)");
		return runScript(
			parsed.script,
			{ out, shotsDir: str(a, "shots_dir") ?? out.replace(/\/[^/]*$/, "") },
			realEnv(),
		);
	},

	async browser_screenshot(a) {
		const t = browserTab(a);
		focusWorktree(t.worktreeId);
		live().ws.activate(t.id);
		await new Promise((r) => setTimeout(r, 600));
		const area = document.querySelector<HTMLElement>(".pane:not(.hidden) .browser .area");
		if (!area) throw new Error("browser tab is not visible");
		const r = area.getBoundingClientRect();
		// `window_region_png` crops in window points; the area is measured in CSS px (they differ under layout zoom).
		const crop = nativeRect({ x: r.left, y: r.top, w: r.width, h: r.height });
		const path = await invoke<string>("window_region_png", { ...crop });
		return { path };
	},
};

/** Serve `atlas wt|term|browser|file` requests forwarded by ctl.rs. */
export async function startCtl() {
	return listen<{ id: number; cmd: string; args: Args }>("ctl://request", async (e) => {
		const { id, cmd, args } = e.payload;
		const h = handlers[cmd];
		try {
			if (!h) throw new Error(`unknown command ${cmd}`);
			const value = await h(args ?? {});
			await invoke("ctl_reply", { id, ok: true, value: value ?? null });
		} catch (err) {
			await invoke("ctl_reply", { id, ok: false, value: errText(err) });
		}
	});
}
