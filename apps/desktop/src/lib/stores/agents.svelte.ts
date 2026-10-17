import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "@codeatlasdev/ui";
import { sound } from "#lib/sound/sound.svelte.ts";
import { agentSound } from "#lib/sound/triggers.ts";
import { api, errText } from "#lib/api.ts";
import { host } from "#lib/host.ts";
import { titleState } from "#lib/title.ts";
import { noticeText, shouldNotify, type TermNotice } from "#lib/termNotice.ts";
import { uid, ws, type Tab } from "#lib/stores/workspace.svelte.ts";
import { activityTracker } from "#lib/stores/agentActivity.ts";

interface AgentInfo {
	id: string;
	label: string;
	installed: boolean;
	path: string | null;
	version: string | null;
	/** A user-defined agent (Settings → Agents → Custom agents), any CLI. */
	custom: boolean;
}

export type AgentState = "idle" | "working" | "blocked" | "waiting" | "done";

interface AgentStatus {
	state: AgentState;
	agent: string;
	session_id: string | null;
	session_file: string | null;
	model: string | null;
	prompt: string | null;
	last_message: string | null;
	updated_at: number;
	state_started_at: number;
	worktree_id: string | null;
	tab_id: string | null;
}

interface Inbox {
	id: string;
	ts: number;
	worktreeId: string;
	tabId: string;
	title: string;
	body: string;
	read: boolean;
}

interface NotifySettings {
	enabled: boolean;
	sound: boolean;
	suppressWhenFocused: boolean;
}

/** Hooks are authoritative; the title fallback only applies after this much hook silence. */
const HOOK_GRACE_MS = 5_000;
const INBOX_MAX = 50;
const PASTE_SETTLE_MS = 150;
const ATTENTION: Record<string, true> = { done: true, waiting: true, blocked: true };

class Agents {
	list = $state<AgentInfo[]>([]);
	defaultAgent = $state("omp");
	/** Live status per pane key (= terminal tab id). */
	status = $state<Record<string, AgentStatus>>({});
	inbox = $state<Inbox[]>([]);
	notify = $state<NotifySettings>({ enabled: true, sound: true, suppressWhenFocused: true });
	ompIntegration = $state<"ok" | "missing" | "unknown">("unknown");
	private focused = true;
	private off: UnlistenFn[] = [];
	/** Working/idle from terminal output, for custom agents (no hooks unless they call `atlas hook`). */
	private activity = activityTracker((pane, state) => {
		const tab = ws.tab(pane);
		if (tab) this.fallback(tab, state);
	});

	async init() {
		const s = await api.settings();
		if (typeof s.defaultAgent === "string") this.defaultAgent = s.defaultAgent;
		if (s.notifications && typeof s.notifications === "object")
			this.notify = { ...this.notify, ...(s.notifications as Partial<NotifySettings>) };
		const win = getCurrentWindow();
		this.focused = await win.isFocused();
		this.off.push(
			await win.onFocusChanged(({ payload }) => {
				this.focused = payload;
				const w = ws.selected;
				if (payload && w?.unread) w.unread = false;
			}),
			await listen<{ pane_key: string; status: AgentStatus }>("agent://status", (e) =>
				this.apply(e.payload.pane_key, e.payload.status),
			),
			await listen<{ tab: string } & TermNotice>("term://notify", (e) => this.termNotice(e.payload)),
			await listen<{ ev: string; pty?: string; title?: string }>("host://event", (e) => {
				const tab = e.payload.pty ? ws.doc.tabs.find((t) => t.pty === e.payload.pty && t.agent) : undefined;
				if (!tab) return;
				if (e.payload.ev === "pty_exit") {
					this.activity.forget(tab.id);
					void invoke("agent_pane_exited", { pane: tab.id });
				}
				if (e.payload.ev === "pty_title") {
					const state = titleState(e.payload.title ?? "");
					if (state) this.fallback(tab, state);
				}
				if (e.payload.ev === "pty_activity" && this.list.find((a) => a.id === tab.agent)?.custom)
					this.activity.bump(tab.id);
			}),
		);
		this.status = await invoke<Record<string, AgentStatus>>("agent_status_all");
		// Restart recovery: panes the hub doesn't know get their state from the title.
		for (const t of ws.doc.tabs)
			if (t.agent && t.pty && !this.status[t.id]) {
				const state = titleState(ws.titles[t.pty] ?? "");
				if (state) this.fallback(t, state);
			}
		this.list = await invoke<AgentInfo[]>("agent_list");
		void this.checkOmp();
	}

	dispose() {
		this.off.forEach((u) => u());
		this.off = [];
	}

	/** State from the terminal (title, output) when the agent's hooks have been silent for `HOOK_GRACE_MS`. */
	private fallback(tab: Tab, state: AgentState) {
		const cur = this.status[tab.id];
		if (cur && Date.now() - cur.updated_at < HOOK_GRACE_MS) return;
		if (cur?.state === state) return;
		this.apply(
			tab.id,
			{
				...(cur ?? {
					agent: tab.agent!,
					session_id: null,
					session_file: null,
					model: null,
					prompt: null,
					last_message: null,
					worktree_id: tab.worktreeId,
					tab_id: tab.id,
				}),
				state,
				state_started_at: Date.now(),
				updated_at: 0,
			},
			true,
		);
	}

	/** OSC 9 / 777 from a terminal: inbox entry, unread dot and (when away) a system banner, like an agent finishing. */
	private termNotice(e: { tab: string } & TermNotice) {
		const tab = ws.tab(e.tab);
		if (!tab) return;
		const w = ws.worktree(tab.worktreeId);
		const tabVisible = ws.doc.selected === tab.worktreeId && ws.group(tab.groupId)?.activeTab === tab.id;
		const away = !this.focused || !tabVisible;
		if (w && away) w.unread = true;
		const { title, body } = noticeText(e, w?.name);
		this.inbox = [
			{ id: uid(), ts: Date.now(), worktreeId: tab.worktreeId, tabId: tab.id, title, body, read: false },
			...this.inbox,
		].slice(0, INBOX_MAX);
		if (shouldNotify(this.notify, { windowFocused: this.focused, tabVisible }))
			void invoke("agent_notify", { title, body, sound: this.notify.sound }).catch(() => {});
	}

	private apply(pane: string, st: AgentStatus, fromTitle = false) {
		const prev = this.status[pane]?.state;
		this.status[pane] = st;
		const tab = ws.tab(pane);
		const worktreeId = tab?.worktreeId ?? st.worktree_id;
		if (!worktreeId || prev === st.state) return;
		ws.pushFeed({
			ts: Date.now(),
			worktreeId,
			tabId: pane,
			agent: st.agent,
			state: st.state,
			text: st.last_message ?? undefined,
		});
		const w = ws.worktree(worktreeId);
		if (w) w.lastActivityAt = Date.now();
		if (fromTitle || prev !== "working" || !ATTENTION[st.state]) return;
		const away = !this.focused || ws.doc.selected !== worktreeId;
		if (w && away) w.unread = true;
		const cue = agentSound(st.state);
		if (cue) sound.play(cue);
		const title = `${w?.name ?? "worktree"} · ${st.agent}`;
		const body =
			st.state === "done" ? (st.last_message ?? "Done").replace(/\s+/g, " ").slice(0, 120) : "Needs you";
		this.inbox = [
			{ id: uid(), ts: Date.now(), worktreeId, tabId: pane, title, body, read: false },
			...this.inbox,
		].slice(0, INBOX_MAX);
		if (this.notify.enabled && (away || !this.notify.suppressWhenFocused))
			void invoke("agent_notify", { title, body, sound: this.notify.sound }).catch(() => {});
	}

	/** Re-read the agent list (after custom agents change in Settings). */
	async refresh() {
		this.list = await invoke<AgentInfo[]>("agent_list");
	}

	async setDefault(id: string) {
		this.defaultAgent = id;
		await api.settingsPatch({ defaultAgent: id });
	}

	async saveNotify() {
		await api.settingsPatch({ notifications: this.notify });
	}

	/** Open an agent terminal in the worktree (focused group unless `groupId`). */
	async launch(
		worktreeId: string,
		agent = this.defaultAgent,
		opts: { prompt?: string; resume?: string; groupId?: string; chat?: boolean } = {},
	) {
		const tabId = uid();
		try {
			const cmd = await invoke<{ argv: string[]; env: Record<string, string>; label: string }>(
				"agent_command",
				{
					agent,
					tabId,
					worktreeId,
					prompt: opts.prompt ?? null,
					resume: opts.resume ?? null,
				},
			);
			// Native chat: omp's rpc-ui protocol over pipes; the first prompt goes through the protocol.
			const chat = opts.chat === true && agent === "omp";
			const argv = chat ? [...cmd.argv.filter((a) => a !== opts.prompt), "--mode", "rpc-ui"] : cmd.argv;
			const t = await ws.newTerminal(worktreeId, {
				tabId,
				chat,
				argv,
				env: cmd.env,
				label: cmd.label,
				agent,
				groupId: opts.groupId,
			});
			if (t && chat && opts.prompt) t.pendingPrompt = opts.prompt;
			// Hooks that arrive without this tab's env (Codex) are matched by agent + cwd.
			const w = ws.worktree(worktreeId);
			if (t && w) void invoke("agent_bind", { pane: t.id, agent, cwd: w.path }).catch(() => {});
			if (t)
				this.status[t.id] = {
					state: "idle",
					agent,
					session_id: null,
					session_file: opts.resume ?? null,
					model: null,
					prompt: opts.prompt ?? null,
					last_message: null,
					updated_at: 0,
					state_started_at: Date.now(),
					worktree_id: worktreeId,
					tab_id: t.id,
				};
			return t;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	/**
	 * Submit text to an agent terminal as a bracketed paste (keeps newlines, no
	 * `@`/`/` autocomplete popups), then Enter once the TUI has taken the paste.
	 */
	async send(tabId: string, text: string) {
		const t = ws.tab(tabId);
		if (!t?.pty) return;
		await host.ptyWrite(t.pty, `\x1b[200~${text.replace(/\x1b/g, "")}\x1b[201~`);
		await new Promise((r) => setTimeout(r, PASTE_SETTLE_MS));
		await host.ptyWrite(t.pty, "\r");
	}

	/** Agent tabs of a worktree, most recently active first. */
	tabsIn(worktreeId: string): Tab[] {
		const recent = ws.doc.recentTabs[worktreeId] ?? [];
		return ws
			.tabsOf(worktreeId)
			.filter((t) => t.agent && t.pty)
			.sort((a, b) => (recent.indexOf(a.id) + 1 || 99) - (recent.indexOf(b.id) + 1 || 99));
	}

	async checkOmp() {
		if (!this.list.find((a) => a.id === "omp")?.installed) return;
		const r = await host
			.exec("/", [
				"sh",
				"-c",
				'test -f "$HOME/.omp/plugins/node_modules/atlas-omp/extensions/atlas-status.ts"',
			])
			.catch(() => null);
		this.ompIntegration = r?.code === 0 ? "ok" : "missing";
	}

	async enableOmp() {
		const r = await host
			.exec("/", ["atlas", "setup", "omp", "--apply"], { timeout_ms: 120_000 })
			.catch((e) => {
				toast.error(errText(e));
				return null;
			});
		if (r && r.code !== 0) toast.error(r.stderr || "atlas setup omp failed");
		await this.checkOmp();
		if (this.ompIntegration === "ok") toast.success("omp integration enabled");
	}
}

export const agents = new Agents();
