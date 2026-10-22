/**
 * Bots store: bot hosts, per-host bot lists, selected bot, and event-driven refresh.
 *
 * All hostd bot commands route through `hostCall` with the host key in args; local
 * bots use no host arg (defaults to the local daemon).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { BotLook } from "#lib/bots/look.ts";
import { hostCall } from "#lib/host.ts";
import { pairUrl, type PairStart } from "#lib/settings/mobile.ts";
import type { AgentIntegration } from "#lib/stores/agentIntegration.ts";
import type {
	Bot,
	BotHost,
	BotProject,
	BotSelection,
	BotTask,
	BotUsageRecord,
	Routine,
	Trigger,
} from "#lib/bots.ts";

export type {
	Bot,
	BotHost,
	BotProject,
	BotSelection,
	BotTask,
	BotUsageRecord,
	Routine,
	Trigger,
	AgentIntegration,
};

interface HostEntry {
	/** `""` for local, `"ssh:atlas-bot@server"` for remote. */
	key: string;
	name: string;
	bots: Bot[];
	/** Whether the last `bot_list` call succeeded. */
	ok: boolean;
}

/** Build the `host` routing arg for `hostCall`; omit entirely for local. */
function hostArgs(key: string): Record<string, unknown> {
	return key ? { host: key } : {};
}

type TimerHandle = ReturnType<typeof setTimeout>;

class BotsStore {
	/** All configured bot computer entries (local always at index 0). */
	hosts = $state<HostEntry[]>([{ key: "", name: "This computer", bots: [], ok: true }]);
	/** Currently selected bot. */
	selected = $state<BotSelection | null>(null);
	/** When set, the next bot creation should link this project automatically. */
	pendingProjectLink = $state<BotProject | null>(null);
	/** Tasks for the selected bot, newest first. */
	tasks = $state<BotTask[]>([]);
	tasksLoading = $state(false);

	private off: UnlistenFn[] = [];
	private refreshTimers = new Map<string, TimerHandle>();
	/** key = "${hostKey}:${botId}", value = last-seen task ts (ms) */
	#lastSeen = $state<Map<string, number>>(new Map());

	// ─── lifecycle ────────────────────────────────────────────────────────────

	async init() {
		const remote = await invoke<BotHost[]>("bot_hosts_list").catch(() => [] as BotHost[]);
		this.hosts = [
			this.hosts[0],
			...remote.map((h) => ({ key: `ssh:${h.host}`, name: h.name, bots: [] as Bot[], ok: true })),
		];

		this.loadUnread();
		await Promise.all(this.hosts.map((h) => this.refreshHost(h.key)));

		this.off.push(
			await listen<{
				ev: string;
				bot_id?: string;
				task?: { id: string; state: string; result?: string; error?: string };
				pane_key?: string;
				tool_name?: string;
			}>("host://event", (e) => void this.onHostEvent(e.payload)),
		);
	}

	dispose() {
		this.off.forEach((u) => u());
		this.off = [];
		this.refreshTimers.forEach((t) => clearTimeout(t));
		this.refreshTimers.clear();
	}

	// ─── host management ──────────────────────────────────────────────────────

	async addHost(host: string, name: string): Promise<void> {
		await invoke("bot_hosts_add", { host, name });
		const key = `ssh:${host}`;
		if (!this.hosts.find((h) => h.key === key)) {
			this.hosts = [...this.hosts, { key, name, bots: [], ok: true }];
		}
		await this.refreshHost(key);
		// Mac→phone invite: push a computer_invite event to paired phones so they can
		// add the bot server without a QR code. Non-critical; ignore failures.
		try {
			const p = await hostCall<PairStart>("mobile_pair_start", { host: key });
			const url = pairUrl(p);
			await hostCall("mobile_invite", { url, name });
		} catch {
			// User can pair manually via Settings → Bots.
		}
	}

	async removeHost(key: string): Promise<void> {
		if (!key) return; // never remove local
		await invoke("bot_hosts_remove", { host: key.replace(/^ssh:/, "") });
		this.hosts = this.hosts.filter((h) => h.key !== key);
		if (this.selected?.hostKey === key) this.selected = null;
	}

	// ─── bot CRUD ─────────────────────────────────────────────────────────────

	async createBot(
		hostKey: string,
		params: { name: string; role: string; agent: string; approval: string; look?: BotLook },
	): Promise<string> {
		const p = params.agent ? params : { ...params, agent: undefined };
		const r = await hostCall<Bot>("bot_create", { ...hostArgs(hostKey), ...p });
		await this.refreshHost(hostKey);
		return r.id;
	}

	async updateBot(
		hostKey: string,
		id: string,
		patch: Partial<{
			name: string;
			role: string;
			agent: string;
			approval: string;
			look: BotLook;
			project: BotProject | null;
		}>,
	): Promise<void> {
		await hostCall("bot_update", { ...hostArgs(hostKey), id, ...patch });
		await this.refreshHost(hostKey);
	}

	async deleteBot(hostKey: string, id: string): Promise<void> {
		await hostCall("bot_delete", { ...hostArgs(hostKey), id });
		if (this.selected?.hostKey === hostKey && this.selected?.botId === id) this.selected = null;
		await this.refreshHost(hostKey);
	}

	// ─── tasks ────────────────────────────────────────────────────────────────

	async sendTask(prompt: string): Promise<void> {
		if (!this.selected) return;
		const { hostKey, botId } = this.selected;
		await hostCall("bot_task", { ...hostArgs(hostKey), id: botId, prompt, source: "desktop" });
		await this.refreshHost(hostKey);
		await this.loadTasks();
	}

	async cancelTask(taskId: string): Promise<void> {
		if (!this.selected) return;
		const { hostKey, botId } = this.selected;
		await hostCall("bot_cancel", { ...hostArgs(hostKey), id: botId, task: taskId });
		await this.loadTasks();
	}

	async loadTasks(): Promise<void> {
		if (!this.selected) return;
		this.tasksLoading = true;
		try {
			const r = await hostCall<{ tasks: BotTask[] }>("bot_tasks", {
				...hostArgs(this.selected.hostKey),
				id: this.selected.botId,
				limit: 100,
			});
			this.tasks = r.tasks;
		} catch {
			// keep stale tasks on error
		} finally {
			this.tasksLoading = false;
		}
	}

	// ─── memory ───────────────────────────────────────────────────────────────

	async getMemory(): Promise<string> {
		if (!this.selected) return "";
		const r = await hostCall<{ text: string }>("bot_memory", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
		});
		return r.text ?? "";
	}

	async setMemory(text: string): Promise<void> {
		if (!this.selected) return;
		await hostCall("bot_memory_set", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
			text,
		});
	}

	// ─── triggers ─────────────────────────────────────────────────────────────

	async setTriggers(triggers: Trigger[]): Promise<void> {
		if (!this.selected) return;
		await hostCall("bot_triggers_set", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
			triggers,
		});
		await this.refreshHost(this.selected.hostKey);
	}

	// (setRoutines removed — call setTriggers with ScheduleTrigger[] instead)

	// ─── ask answers ──────────────────────────────────────────────────────────

	/**
	 * Answer a pending ask card. Pass `key` (option key) XOR `text` (free-text).
	 * Wire: BotAnswer { id, ask_id, key } or { id, ask_id, text }
	 */
	async botAnswer(askId: string, opts: { key?: string; text?: string }): Promise<void> {
		if (!this.selected) return;
		await hostCall("bot_answer", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
			ask_id: askId,
			...opts,
		});
		void this.loadTasks();
	}

	// ─── usage ────────────────────────────────────────────────────────────────

	/** Returns `{ records: BotUsageRecord[] }`. `id` omitted = all bots on host. */
	async getUsage(opts?: { since?: number }): Promise<{ records: BotUsageRecord[] }> {
		if (!this.selected) return { records: [] };
		return hostCall<{ records: BotUsageRecord[] }>("bot_usage", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
			...(opts?.since ? { since: opts.since } : {}),
		});
	}

	// ─── secrets ──────────────────────────────────────────────────────────────

	/** Set a secret env var for the bot engine subprocess (desktop-only). */
	async setSecret(key: string, value: string): Promise<void> {
		if (!this.selected) return;
		await hostCall("bot_secret_set", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.botId,
			key,
			value,
		});
	}

	// ─── approvals ────────────────────────────────────────────────────────────

	async approve(paneKey: string, req: string, allow: boolean): Promise<void> {
		if (!this.selected) return;
		await hostCall("agent_approve", { ...hostArgs(this.selected.hostKey), pane: paneKey, req, allow });
		this.scheduleRefresh(this.selected.hostKey, 500);
	}

	// ─── telegram ─────────────────────────────────────────────────────────────

	async setTelegramToken(token: string | null): Promise<void> {
		if (!this.selected) return;
		await hostCall("bot_telegram_set", { ...hostArgs(this.selected.hostKey), token });
	}

	async telegramLinkCode(): Promise<string> {
		if (!this.selected) return "";
		const r = await hostCall<{ code: string }>("bot_telegram_link", hostArgs(this.selected.hostKey));
		return r.code;
	}

	// ─── selection ────────────────────────────────────────────────────────────

	async select(hostKey: string, botId: string): Promise<void> {
		const entry = this.hosts.find((h) => h.key === hostKey);
		this.selected = entry ? { hostKey, botId, hostName: entry.name } : null;
		if (this.selected) await this.loadTasks();
		if (this.selected) this.markRead(hostKey, botId);
	}

	get selectedBot(): Bot | undefined {
		if (!this.selected) return undefined;
		return this.hosts
			.find((h) => h.key === this.selected!.hostKey)
			?.bots.find((b) => b.id === this.selected!.botId);
	}

	/** All bots across all hosts, flat. */
	get allBots(): Array<{ hostKey: string; hostName: string; bot: Bot }> {
		return this.hosts.flatMap((h) => h.bots.map((bot) => ({ hostKey: h.key, hostName: h.name, bot })));
	}

	get needsYouCount(): number {
		return this.allBots.filter((b) => b.bot.state === "needs_you").length;
	}

	/** Returns true when a bot has tasks newer than last seen. */
	hasUnread(hostKey: string, botId: string): boolean {
		const bot = this.hosts.find((h) => h.key === hostKey)?.bots.find((b) => b.id === botId);
		if (!bot?.last?.ts) return false;
		const seen = this.#lastSeen.get(`${hostKey}:${botId}`) ?? 0;
		return bot.last.ts * 1000 > seen;
	}

	/** Call when the user opens a bot. Marks all current tasks as seen. */
	markRead(hostKey: string, botId: string): void {
		const bot = this.hosts.find((h) => h.key === hostKey)?.bots.find((b) => b.id === botId);
		const ts = bot?.last?.ts ? bot.last.ts * 1000 : Date.now();
		const mapKey = `${hostKey}:${botId}`;
		this.#lastSeen.set(mapKey, ts);
		try {
			localStorage.setItem(this.lsKey(hostKey, botId), String(ts));
		} catch {
			/* storage full */
		}
	}

	/** Last-seen ts (ms) for the selected bot — passed to BotView for the unread divider. */
	get selectedLastSeen(): number {
		if (!this.selected) return 0;
		return this.#lastSeen.get(`${this.selected.hostKey}:${this.selected.botId}`) ?? 0;
	}

	/** Flat activity feed: all bots' last task, newest first, capped at 100. */
	get activityFeed(): Array<{
		botId: string;
		botName: string;
		hostKey: string;
		hostName: string;
		task: BotTask;
	}> {
		const entries: Array<{
			botId: string;
			botName: string;
			hostKey: string;
			hostName: string;
			task: BotTask;
		}> = [];
		for (const { bot, hostKey, hostName } of this.allBots) {
			if (!bot.last) continue;
			entries.push({
				botId: bot.id,
				botName: bot.name,
				hostKey,
				hostName,
				task: {
					id: `${bot.id}:last`,
					ts: bot.last.ts * 1000,
					source: "desktop" as const,
					prompt: "",
					state: bot.last.state,
					result: bot.last.text,
				},
			});
		}
		return entries.sort((a, b) => b.task.ts - a.task.ts).slice(0, 100);
	}

	/** Fetch agent integrations from a specific host (used by the New Bot dialog). */
	async getHostIntegrations(hostKey: string): Promise<AgentIntegration[]> {
		const r = await hostCall<{ agents: AgentIntegration[] }>("agent_integrations", hostArgs(hostKey));
		return r.agents ?? [];
	}

	/** Whether the last refresh for a host succeeded. */
	hostOk(hostKey: string): boolean {
		return this.hosts.find((h) => h.key === hostKey)?.ok ?? false;
	}

	/**
	 * Inject a synthetic ask-card task into the current bot's task list (for screenshot seeding).
	 * Does NOT persist to hostd. Call from the desktop agent's seed script.
	 */
	injectAskTask(payload: {
		id: string;
		question: string;
		options: Array<{ key: string; title: string; subtitle?: string }>;
	}): void {
		const synthetic: BotTask = {
			id: payload.id,
			ts: Date.now(),
			source: "desktop",
			prompt: "",
			state: "done",
			result: JSON.stringify({ ask: payload }),
		};
		this.tasks = [synthetic, ...this.tasks];
	}

	// ─── internal ─────────────────────────────────────────────────────────────

	private lsKey(hostKey: string, botId: string): string {
		return `bots:seen:${hostKey}:${botId}`;
	}

	private loadUnread(): void {
		try {
			for (let i = 0; i < localStorage.length; i++) {
				const k = localStorage.key(i);
				if (!k?.startsWith("bots:seen:")) continue;
				const v = Number(localStorage.getItem(k));
				if (v > 0) this.#lastSeen.set(k.slice("bots:seen:".length), v);
			}
		} catch {
			/* private mode */
		}
	}

	private async refreshHost(key: string): Promise<void> {
		const idx = this.hosts.findIndex((h) => h.key === key);
		if (idx < 0) return;
		try {
			const r = await hostCall<{ bots: Bot[] }>("bot_list", hostArgs(key));
			this.hosts[idx] = { ...this.hosts[idx], bots: r.bots, ok: true };
		} catch {
			this.hosts[idx] = { ...this.hosts[idx], ok: false };
		}
	}

	private scheduleRefresh(key: string, delayMs: number) {
		const mapKey = key || "local";
		clearTimeout(this.refreshTimers.get(mapKey));
		const t = setTimeout(() => {
			this.refreshTimers.delete(mapKey);
			void this.refreshHost(key);
		}, delayMs);
		this.refreshTimers.set(mapKey, t);
	}

	private async onHostEvent(payload: {
		ev: string;
		bot_id?: string;
		task?: { id: string; state: string; result?: string; error?: string };
		pane_key?: string;
		tool_name?: string;
	}) {
		if (payload.ev === "bot_task") {
			const { bot_id: botId, task } = payload;
			if (task && (task.state === "done" || task.state === "failed")) {
				const entry = this.allBots.find((b) => b.bot.id === botId);
				const name = entry?.bot.name ?? "Bot";
				const body =
					task.state === "done"
						? (task.result ?? "Task done").replace(/\s+/g, " ").slice(0, 120)
						: (task.error ?? "Task failed");
				void invoke("agent_notify", { title: `${name} · ${task.state}`, body, sound: true }).catch(() => {});
			}
			const ownerKey = this.allBots.find((b) => b.bot.id === botId)?.hostKey ?? "";
			this.scheduleRefresh(ownerKey, 200);
			if (this.selected?.botId === botId) void this.loadTasks();
			return;
		}

		if (payload.ev === "bot_updated") {
			// Bot self-updated via set_bot (e.g. engine onboarding changed name/look).
			const ownerKey = this.allBots.find((b) => b.bot.id === payload.bot_id)?.hostKey ?? "";
			this.scheduleRefresh(ownerKey, 100);
			if (this.selected?.botId === payload.bot_id) void this.loadTasks();
			return;
		}

		if (payload.ev === "agent_approval" && payload.pane_key?.startsWith("bot:")) {
			const botId = payload.pane_key.split(":")[1];
			const entry = this.allBots.find((b) => b.bot.id === botId);
			void invoke("agent_notify", {
				title: `${entry?.bot.name ?? "Bot"} needs you`,
				body: `Wants to run ${payload.tool_name ?? "a tool"}`,
				sound: true,
			}).catch(() => {});
			this.scheduleRefresh(entry?.hostKey ?? "", 200);
		}
	}
}

export const bots = new BotsStore();
