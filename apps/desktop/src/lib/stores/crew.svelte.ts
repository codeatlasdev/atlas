/**
 * Atlas Crew store: crew servers, each server's crewmates, the selected crewmate, its tasks and event-driven refresh.
 *
 * Every hostd crewmate/crew command routes through `hostCall` with the server's host key in args; the local
 * daemon (key `""`) takes no host arg.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CrewmateLook } from "#lib/crew/look.ts";
import { hostCall } from "#lib/host.ts";
import { errText } from "#lib/api.ts";
import { normalizeCrewmate, normalizeTask } from "#lib/crew/wire.ts";
import { crewmateKey } from "#lib/crew/crewmateKey.ts";
import { isUnread, loadSeen, saveSeen } from "#lib/crew/unread.ts";
import { pairUrl, type PairStart } from "#lib/settings/mobile.ts";
import type { AgentIntegration } from "#lib/stores/agentIntegration.ts";
import type {
	Crewmate,
	CrewServer,
	CrewmateProject,
	CrewmateSelection,
	CrewmateTask,
	CrewmateUsageRecord,
	Routine,
	Trigger,
} from "#lib/crew.ts";

export type {
	Crewmate,
	CrewServer,
	CrewmateProject,
	CrewmateSelection,
	CrewmateTask,
	CrewmateUsageRecord,
	Routine,
	Trigger,
	AgentIntegration,
};

interface ServerEntry {
	/** `""` for local, `"ssh:atlas-crew@server"` for remote. */
	key: string;
	name: string;
	crewmates: Crewmate[];
	/** Whether the last `crewmate_list` call succeeded. */
	ok: boolean;
	/** Why the last `crewmate_list` failed (shown in the sidebar with Retry). */
	error?: string;
}

/** Build the `host` routing arg for `hostCall`; omit entirely for local. */
function hostArgs(key: string): Record<string, unknown> {
	return key ? { host: key } : {};
}

type TimerHandle = ReturnType<typeof setTimeout>;

class CrewStore {
	/** All configured crew server entries (local always at index 0). */
	servers = $state<ServerEntry[]>([{ key: "", name: "This computer", crewmates: [], ok: true }]);
	/** Currently selected crewmate. */
	selected = $state<CrewmateSelection | null>(null);
	/** The sidebar's order (first mate, then the list as shown), published by CrewWorkspace for ⌘1…⌘9. */
	sidebarOrder = $state<{ hostKey: string; crewmateId: string }[]>([]);
	/** A task the chat should scroll to and flash once it shows (set by a Board card); cleared by the chat. */
	focusTask = $state<string | null>(null);
	/** When set, the next crewmate creation should link this project automatically. */
	pendingProjectLink = $state<CrewmateProject | null>(null);
	/** Tasks for the selected crewmate, newest first. */
	tasks = $state<CrewmateTask[]>([]);
	tasksLoading = $state(false);
	/** Tasks for ALL crewmates, keyed by `crewmateKey(hostKey, crewmateId)` — drives the board view. */
	boardTasks = $state<Map<string, CrewmateTask[]>>(new Map());
	/** True while CrewBoard is mounted; gate for per-crewmate refresh on crewmate_task events. */
	boardOpen = $state(false);

	private off: UnlistenFn[] = [];
	private refreshTimers = new Map<string, TimerHandle>();
	/** key = `crewmateKey`, value = last-seen task ts (ms) */
	#lastSeen = $state<Map<string, number>>(new Map());

	// ─── lifecycle ────────────────────────────────────────────────────────────

	async init() {
		const remote = await invoke<CrewServer[]>("crew_servers_list").catch(() => [] as CrewServer[]);
		this.servers = [
			this.servers[0],
			...remote.map((h) => ({ key: `ssh:${h.host}`, name: h.name, crewmates: [] as Crewmate[], ok: true })),
		];

		this.#lastSeen = loadSeen(localStorage);
		await Promise.all(this.servers.map((h) => this.refreshHost(h.key)));

		this.off.push(
			await listen<{
				ev: string;
				crewmate_id?: string;
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

	async addServer(host: string, name: string): Promise<void> {
		await invoke("crew_servers_add", { host, name });
		const key = `ssh:${host}`;
		if (!this.servers.find((h) => h.key === key)) {
			this.servers = [...this.servers, { key, name, crewmates: [], ok: true }];
		}
		await this.refreshHost(key);
		// Mac→phone invite: push a computer_invite event to paired phones so they can
		// add the crewmate server without a QR code. Non-critical; ignore failures.
		try {
			const p = await hostCall<PairStart>("mobile_pair_start", { host: key });
			const url = pairUrl(p);
			await hostCall("mobile_invite", { url, name });
		} catch {
			// User can pair manually via Settings → Atlas Crew.
		}
	}

	async removeServer(key: string): Promise<void> {
		if (!key) return; // never remove local
		await invoke("crew_servers_remove", { host: key.replace(/^ssh:/, "") });
		this.servers = this.servers.filter((h) => h.key !== key);
		if (this.selected?.hostKey === key) this.selected = null;
	}

	// ─── crewmate CRUD ─────────────────────────────────────────────────────────────

	async createCrewmate(
		hostKey: string,
		params: {
			name: string;
			role: string;
			agent: string;
			approval: string;
			look?: CrewmateLook;
			setup_step?: string;
		},
	): Promise<string> {
		const p = params.agent ? params : { ...params, agent: undefined };
		const r = await hostCall<Crewmate>("crewmate_create", { ...hostArgs(hostKey), ...p });
		await this.refreshHost(hostKey);
		return r.id;
	}

	async updateCrewmate(
		hostKey: string,
		id: string,
		patch: Partial<{
			name: string;
			role: string;
			agent: string;
			approval: string;
			look: CrewmateLook;
			project: CrewmateProject | null;
			/** Pass `null` to clear the setup step (mark setup complete). */
			setup_step: string | null;
		}>,
	): Promise<void> {
		await hostCall("crewmate_update", { ...hostArgs(hostKey), id, ...patch });
		await this.refreshHost(hostKey);
	}

	async deleteCrewmate(hostKey: string, id: string): Promise<void> {
		await hostCall("crewmate_delete", { ...hostArgs(hostKey), id });
		if (this.selected?.hostKey === hostKey && this.selected?.crewmateId === id) this.selected = null;
		await this.refreshHost(hostKey);
	}

	// ─── tasks ────────────────────────────────────────────────────────────────

	async sendTask(prompt: string): Promise<void> {
		if (!this.selected) return;
		const { hostKey, crewmateId } = this.selected;
		const tz = Intl.DateTimeFormat().resolvedOptions().timeZone;
		await hostCall("crewmate_task", { ...hostArgs(hostKey), id: crewmateId, prompt, source: "desktop", tz });
		await this.refreshHost(hostKey);
		await this.loadTasks();
	}

	async cancelTask(taskId: string): Promise<void> {
		if (!this.selected) return;
		const { hostKey, crewmateId } = this.selected;
		await hostCall("crewmate_cancel", { ...hostArgs(hostKey), id: crewmateId, task: taskId });
		await this.loadTasks();
	}

	/** One crewmate's tasks from hostd, newest first, timestamps already in ms. */
	private async fetchTasks(hostKey: string, crewmateId: string, limit: number): Promise<CrewmateTask[]> {
		const r = await hostCall<{ tasks: CrewmateTask[] }>("crewmate_tasks", {
			...hostArgs(hostKey),
			id: crewmateId,
			limit,
		});
		return r.tasks.map(normalizeTask);
	}

	async loadTasks(): Promise<void> {
		if (!this.selected) return;
		this.tasksLoading = true;
		try {
			this.tasks = await this.fetchTasks(this.selected.hostKey, this.selected.crewmateId, 100);
		} catch {
			// keep stale tasks on error
		} finally {
			this.tasksLoading = false;
		}
	}

	// ─── board tasks ──────────────────────────────────────────────────────────

	/** Load tasks for every crewmate on every host (populates `boardTasks`). Called when the board mounts. */
	async loadBoardTasks(): Promise<void> {
		const results = await Promise.allSettled(
			this.allCrewmates.map(async ({ hostKey, crewmate }) => ({
				key: crewmateKey(hostKey, crewmate.id),
				tasks: await this.fetchTasks(hostKey, crewmate.id, 50),
			})),
		);
		const next = new Map(this.boardTasks);
		for (const r of results) {
			if (r.status === "fulfilled") next.set(r.value.key, r.value.tasks);
		}
		this.boardTasks = next;
	}

	/** Refresh one crewmate's board tasks when a `crewmate_task` event fires while the board is open. */
	private async refreshCrewmateBoardTasks(crewmateId: string): Promise<void> {
		const entry = this.allCrewmates.find((b) => b.crewmate.id === crewmateId);
		if (!entry) return;
		const { hostKey, crewmate } = entry;
		try {
			const next = new Map(this.boardTasks);
			next.set(crewmateKey(hostKey, crewmate.id), await this.fetchTasks(hostKey, crewmate.id, 50));
			this.boardTasks = next;
		} catch {
			// keep stale data on error
		}
	}

	/** Flat list of {hostKey, crewmate, task} entries for all crewmates — input to boardColumns(). */
	get boardEntries(): Array<{ hostKey: string; crewmate: Crewmate; task: CrewmateTask }> {
		return this.allCrewmates.flatMap(({ hostKey, crewmate }) =>
			(this.boardTasks.get(crewmateKey(hostKey, crewmate.id)) ?? []).map((task) => ({
				hostKey,
				crewmate,
				task,
			})),
		);
	}

	// ─── memory ───────────────────────────────────────────────────────────────

	async getMemory(): Promise<string> {
		if (!this.selected) return "";
		const r = await hostCall<{ text: string }>("crewmate_memory", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.crewmateId,
		});
		return r.text ?? "";
	}

	async setMemory(text: string): Promise<void> {
		if (!this.selected) return;
		await hostCall("crewmate_memory_set", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.crewmateId,
			text,
		});
	}

	// ─── triggers ─────────────────────────────────────────────────────────────

	async setTriggers(triggers: Trigger[]): Promise<void> {
		if (!this.selected) return;
		await hostCall("crewmate_triggers_set", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.crewmateId,
			triggers,
		});
		await this.refreshHost(this.selected.hostKey);
	}

	// (setRoutines removed — call setTriggers with ScheduleTrigger[] instead)

	// ─── ask answers ──────────────────────────────────────────────────────────

	/**
	 * Answer a pending ask card. Pass `key` (option key) XOR `text` (free-text).
	 * Wire: CrewmateAnswer { id, ask_id, key } or { id, ask_id, text }
	 */
	async crewmateAnswer(askId: string, opts: { key?: string; text?: string }): Promise<void> {
		if (!this.selected) return;
		await hostCall("crewmate_answer", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.crewmateId,
			ask_id: askId,
			...opts,
		});
		void this.loadTasks();
	}

	// ─── usage ────────────────────────────────────────────────────────────────

	/** Returns `{ records: CrewmateUsageRecord[] }`. `id` omitted = all crewmates on host. */
	async getUsage(opts?: { since?: number }): Promise<{ records: CrewmateUsageRecord[] }> {
		if (!this.selected) return { records: [] };
		return hostCall<{ records: CrewmateUsageRecord[] }>("crewmate_usage", {
			...hostArgs(this.selected.hostKey),
			id: this.selected.crewmateId,
			...(opts?.since ? { since: opts.since } : {}),
		});
	}

	// ─── approvals ────────────────────────────────────────────────────────────

	async approve(paneKey: string, req: string, allow: boolean): Promise<void> {
		if (!this.selected) return;
		await hostCall("agent_approve", { ...hostArgs(this.selected.hostKey), pane: paneKey, req, allow });
		this.scheduleRefresh(this.selected.hostKey, 500);
	}

	// ─── selection ────────────────────────────────────────────────────────────

	async select(hostKey: string, crewmateId: string): Promise<void> {
		const entry = this.servers.find((h) => h.key === hostKey);
		this.selected = entry ? { hostKey, crewmateId, hostName: entry.name } : null;
		if (this.selected) await this.loadTasks();
		if (this.selected) this.markRead(hostKey, crewmateId);
	}

	/** ⌘n in the Crew workspace: the n-th crewmate of the sidebar (1-based); `last` for ⌘9. */
	selectAt(n: number | "last"): void {
		const e = n === "last" ? this.sidebarOrder.at(-1) : this.sidebarOrder[n - 1];
		if (e) void this.select(e.hostKey, e.crewmateId);
	}

	get selectedCrewmate(): Crewmate | undefined {
		if (!this.selected) return undefined;
		return this.servers
			.find((h) => h.key === this.selected!.hostKey)
			?.crewmates.find((b) => b.id === this.selected!.crewmateId);
	}

	/** All crewmates across all hosts, flat. */
	get allCrewmates(): Array<{ hostKey: string; hostName: string; crewmate: Crewmate }> {
		return this.servers.flatMap((h) =>
			h.crewmates.map((crewmate) => ({ hostKey: h.key, hostName: h.name, crewmate })),
		);
	}

	get needsYouCount(): number {
		return this.allCrewmates.filter((b) => b.crewmate.state === "needs_you").length;
	}

	/** Returns true when a crewmate has tasks newer than last seen. */
	hasUnread(hostKey: string, crewmateId: string): boolean {
		const crewmate = this.servers.find((h) => h.key === hostKey)?.crewmates.find((b) => b.id === crewmateId);
		return isUnread(crewmate?.last?.ts, this.#lastSeen.get(crewmateKey(hostKey, crewmateId)));
	}

	/** Call when the user opens a crewmate. Marks all current tasks as seen. */
	markRead(hostKey: string, crewmateId: string): void {
		const crewmate = this.servers.find((h) => h.key === hostKey)?.crewmates.find((b) => b.id === crewmateId);
		const ts = crewmate?.last?.ts ?? Date.now();
		this.#lastSeen.set(crewmateKey(hostKey, crewmateId), ts);
		saveSeen(localStorage, hostKey, crewmateId, ts);
	}

	/** Last-seen ts (ms) for the selected crewmate — passed to CrewmateView for the unread divider. */
	get selectedLastSeen(): number {
		if (!this.selected) return 0;
		return this.#lastSeen.get(crewmateKey(this.selected.hostKey, this.selected.crewmateId)) ?? 0;
	}
	/** Fetch agent integrations from a specific host (used by the New Crewmate dialog). */
	async getServerIntegrations(hostKey: string): Promise<AgentIntegration[]> {
		const r = await hostCall<{ agents: AgentIntegration[] }>("agent_integrations", hostArgs(hostKey));
		return r.agents ?? [];
	}

	/** Whether the last refresh for a host succeeded. */
	serverOk(hostKey: string): boolean {
		return this.servers.find((h) => h.key === hostKey)?.ok ?? false;
	}
	// ─── internal ─────────────────────────────────────────────────────────────

	private async refreshHost(key: string): Promise<void> {
		const idx = this.servers.findIndex((h) => h.key === key);
		if (idx < 0) return;
		try {
			const r = await hostCall<{ crewmates: Crewmate[] }>("crewmate_list", hostArgs(key));
			const crewmates = r.crewmates.map(normalizeCrewmate);
			this.servers[idx] = { ...this.servers[idx], crewmates, ok: true, error: undefined };
		} catch (e) {
			// Keep the crewmates we last saw: a crew server that restarted (redeploy, reboot) must not make the crew vanish
			// from the sidebar; say why instead, and the next refresh brings it back.
			this.servers[idx] = { ...this.servers[idx], ok: false, error: errText(e) };
		}
	}

	/** Re-read every crew server that failed last time (window refocus, a Retry button). */
	async retryFailed(): Promise<void> {
		await Promise.all(this.servers.filter((h) => !h.ok).map((h) => this.refreshHost(h.key)));
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
		crewmate_id?: string;
		task?: { id: string; state: string; result?: string; error?: string };
		pane_key?: string;
		tool_name?: string;
	}) {
		if (payload.ev === "crewmate_task") {
			const { crewmate_id: crewmateId, task } = payload;
			if (task && (task.state === "done" || task.state === "failed")) {
				const entry = this.allCrewmates.find((b) => b.crewmate.id === crewmateId);
				const name = entry?.crewmate.name ?? "Crewmate";
				const body =
					task.state === "done"
						? (task.result ?? "Task done").replace(/\s+/g, " ").slice(0, 120)
						: (task.error ?? "Task failed");
				void invoke("agent_notify", { title: `${name} · ${task.state}`, body, sound: true }).catch(() => {});
			}
			const ownerKey = this.allCrewmates.find((b) => b.crewmate.id === crewmateId)?.hostKey ?? "";
			this.scheduleRefresh(ownerKey, 200);
			if (this.selected?.crewmateId === crewmateId) void this.loadTasks();
			if (this.boardOpen && crewmateId) void this.refreshCrewmateBoardTasks(crewmateId);
			return;
		}

		if (payload.ev === "crewmate_updated") {
			// Crewmate self-updated via set_profile (e.g. engine onboarding changed name/look).
			const ownerKey = this.allCrewmates.find((b) => b.crewmate.id === payload.crewmate_id)?.hostKey ?? "";
			this.scheduleRefresh(ownerKey, 100);
			if (this.selected?.crewmateId === payload.crewmate_id) void this.loadTasks();
			return;
		}

		if (payload.ev === "agent_approval" && payload.pane_key?.startsWith("crew:")) {
			const crewmateId = payload.pane_key.split(":")[1];
			const entry = this.allCrewmates.find((b) => b.crewmate.id === crewmateId);
			void invoke("agent_notify", {
				title: `${entry?.crewmate.name ?? "Crewmate"} needs you`,
				body: `Wants to run ${payload.tool_name ?? "a tool"}`,
				sound: true,
			}).catch(() => {});
			this.scheduleRefresh(entry?.hostKey ?? "", 200);
		}
	}
}

export const crew = new CrewStore();
