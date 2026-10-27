import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "@codeatlasdev/ui";
import { api, errText } from "#lib/api.ts";
import {
	resolveSection,
	type SettingsSection,
	type SettingsTab,
	type SettingsTarget,
} from "#lib/settings/index.ts";
import { sound } from "#lib/sound/sound.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import {
	CHECK_EVERY_MS,
	EMPTY_PREFS,
	EMPTY_PROGRESS,
	blurbOf,
	finishOutcome,
	finishToast,
	justUpdated,
	parseChannelState,
	parseChannelViews,
	parseNotes,
	parsePrefs,
	phaseFromLog,
	reduceProgress,
	selectNotes,
	shouldNotify,
	shouldRecheck,
	showPill,
	stepsFrom,
	type Channel,
	type ChannelAction,
	type ChannelState,
	type ChannelView,
	type InstallOutcome,
	type ProgressEvent,
	type ReleaseNotes,
	type UpdatePrefs,
} from "#lib/updates.ts";

interface SystemInfo {
	app: { version: string; bundle: string | null; linked_to: string | null; dev: boolean };
	cli: { path: string | null; version: string | null; linked_to: string | null; error: string | null };
}

export interface Release {
	version: string;
	url: string;
	published_at: string;
	notes: string;
	/** The channel `version` was cut for. */
	channel: Channel;
	/** What installing `version` is for this app (`update` only when it is strictly newer). */
	action: ChannelAction;
	downgrade: boolean;
	/** Computed in Rust (SemVer precedence): installed CLI / this app is older than `version`. */
	cli_behind: boolean;
	app_behind: boolean;
	/** Installed CLI / this app is newer than `version` (only an explicit downgrade installs it). */
	cli_ahead: boolean;
	app_ahead: boolean;
}

export type { SettingsSection, SettingsTab } from "#lib/settings/index.ts";

type Run = "update" | "reinstall" | "unlink";

class Updates {
	info = $state<SystemInfo | null>(null);
	release = $state<Release | null>(null);
	error = $state<string | null>(null);
	checking = $state(false);
	running = $state<null | Run>(null);
	/** How the last install ended; drives the step list. */
	outcome = $state<InstallOutcome>("running");
	log = $state<string[]>([]);
	/** Why the last install failed (the CLI's last line); null until it does. */
	failure = $state<string | null>(null);
	/** Structured install progress (phases, download bytes, terminal-host outcome). */
	progress = $state({ ...EMPTY_PROGRESS });
	/** Bytes the pending update downloads (null while unknown). */
	size = $state<number | null>(null);
	relaunchNeeded = $state(false);
	open = $state(false);
	section = $state<SettingsSection>("general");
	/** Tab of the open section; meaningless (and ignored) when the section has no tabs. */
	tab = $state<SettingsTab>("editor");
	/** Row id Settings scrolls to and flashes once its page renders; cleared by the dialog when done. */
	reveal = $state<string | null>(null);
	/** Search box text; lives here so it survives reopening and so `show` can clear it. */
	query = $state("");
	checkedAt = $state<number | null>(null);
	prefs = $state<UpdatePrefs>({ ...EMPTY_PREFS });

	/** Update channel (shared with the CLI through `~/.atlas/update-channel`, never in settings.json). */
	channelState = $state<ChannelState | null>(null);
	/** The three cards with what each offers now; empty until the first (successful) fetch. */
	channels = $state<ChannelView[]>([]);
	channelsLoading = $state(false);
	/** Set when the last `update_channels` failed (offline): the cards render from copy alone. */
	channelsError = $state<string | null>(null);

	/** "What's new" sheet. */
	whatsNewOpen = $state(false);
	notes = $state<ReleaseNotes[]>([]);
	notesLoading = $state(false);
	notesError = $state<string | null>(null);
	/** "Updated to X" card shown once after an update. */
	updatedTo = $state<string | null>(null);

	/** Go to a section (and tab, and row) of Settings. A section without a tab target keeps the current tab. */
	select = (t: SettingsTarget) => {
		this.section = t.section;
		if (t.tab) this.tab = t.tab;
		this.reveal = t.item ?? null;
		this.query = "";
	};

	/**
	 * Open Settings on whatever `target` names: a section, `section/tab`, a row id (the row is revealed) or an id
	 * an older version used. See `resolveSection`.
	 */
	show = (target?: string) => {
		this.select(target === undefined ? { section: this.section } : resolveSection(target));
		this.open = true;
		if (this.section === "general") this.check();
	};

	/** Dev builds and `atlas link` must not be overwritten by a release. */
	linked = $derived(!!(this.info?.cli.linked_to || this.info?.app.linked_to));
	/** A debug/dev app (cargo tauri dev) or a linked checkout. */
	dev = $derived(this.linked || !!this.info?.app.dev);
	cliBehind = $derived(this.release?.cli_behind ?? false);
	appBehind = $derived(this.release?.app_behind ?? false);
	/** Only a strictly newer release is an update: a lower channel's offer is never one. */
	available = $derived(this.cliBehind || this.appBehind);
	/** The followed channel's newest offer is older than what runs: explained, never installed unasked. */
	aheadOfChannel = $derived(this.release?.downgrade ?? false);
	/** The pill in the sidebar: available, not skipped, not a dev build. */
	pill = $derived(showPill(this.available, this.dev, this.prefs, this.release?.version));
	steps = $derived(stepsFrom(this.progress, this.outcome, this.relaunchNeeded));

	#started = false;
	#unlisten: UnlistenFn[] = [];
	#timer: number | null = null;
	/** The running install speaks the `@@atlas` protocol: do not also guess phases from its text. */
	#structured = false;

	async init() {
		if (this.#started) return;
		this.#started = true;
		this.prefs = parsePrefs((await api.settings()).updates);
		await this.loadChannelState();
		this.#unlisten.push(
			await listen<string>("updates://log", (e) => {
				this.log.push(e.payload);
				if (this.log.length > 500) this.log.splice(0, this.log.length - 500);
				// An older CLI has no `@@atlas` protocol: its plain lines still advance the phases.
				const p = phaseFromLog(e.payload);
				if (p && !this.#structured) this.progress = reduceProgress(this.progress, p);
			}),
			await listen<ProgressEvent>("updates://progress", (e) => {
				this.#structured = true;
				this.progress = reduceProgress(this.progress, e.payload);
			}),
			await getCurrentWindow().onFocusChanged(({ payload }) => {
				if (payload && shouldRecheck(this.prefs, this.checkedAt, Date.now())) void this.check();
			}),
		);
		this.#timer = window.setInterval(() => {
			if (this.prefs.auto) void this.check();
		}, CHECK_EVERY_MS);
		await this.check();
		await this.#afterUpdate();
	}

	dispose() {
		this.#unlisten.forEach((u) => u());
		this.#unlisten = [];
		clearInterval(this.#timer ?? undefined);
		this.#timer = null;
		this.#started = false;
	}

	async #save(patch: Partial<UpdatePrefs>) {
		this.prefs = { ...this.prefs, ...patch };
		await api.settingsPatch({ updates: $state.snapshot(this.prefs) });
	}

	/** First launch of a newer version: remember it and offer the notes once. */
	async #afterUpdate() {
		const running = this.info?.app.version;
		if (!running) return;
		if (justUpdated(this.prefs, running)) this.updatedTo = running;
		if (this.prefs.lastSeen !== running) await this.#save({ lastSeen: running });
	}

	async check() {
		if (this.checking) return;
		this.checking = true;
		this.error = null;
		try {
			this.info = await invoke<SystemInfo>("system_info");
			await this.loadChannelState();
			this.release = await invoke<Release>("latest_release");
			this.checkedAt = Date.now();
			void this.#loadSize();
			await this.#notify();
		} catch (e) {
			this.error = errText(e);
		} finally {
			this.checking = false;
		}
	}

	async loadChannelState() {
		try {
			this.channelState = parseChannelState(await invoke("update_channel_get"));
		} catch (e) {
			this.error = errText(e);
		}
	}

	/** The cards with live offers. A failure leaves the copy-only cards and sets `channelsError`. */
	async loadChannels() {
		if (this.channelsLoading) return;
		this.channelsLoading = true;
		this.channelsError = null;
		try {
			await this.loadChannelState();
			this.channels = parseChannelViews(await invoke("update_channels"));
		} catch (e) {
			this.channelsError = errText(e);
			// Keep what was shown; on a cold start fall back to the copy so the cards still render.
			if (!this.channels.length) {
				this.channels = parseChannelViews(await invoke("update_channels_offline").catch(() => []));
			}
		} finally {
			this.channelsLoading = false;
		}
	}

	/** Follow `channel` (persisted for the CLI too), then look again. Never installs anything. */
	setChannel = async (channel: Channel): Promise<boolean> => {
		try {
			this.channelState = parseChannelState(await invoke("update_channel_set", { channel }));
		} catch (e) {
			toast.error(errText(e));
			return false;
		}
		this.notes = [];
		await Promise.all([this.check(), this.loadChannels()]);
		return true;
	};

	/** Download size of the pending update, for the sheet's button. Best effort. */
	async #loadSize() {
		const r = this.release;
		this.size = null;
		if (!r || !this.available) return;
		this.size = await invoke<number | null>("update_size", {
			version: r.version,
			cli: r.cli_behind,
			app: r.app_behind,
		}).catch(() => null);
	}

	/** Native notification once per version, through the same path (and switch) as agent alerts. */
	async #notify() {
		const v = this.release?.version;
		if (!v || !this.pill) return;
		if (!shouldNotify(this.prefs, v, { notificationsOn: agents.notify.enabled, linked: this.dev })) return;
		await this.#save({ notified: v });
		sound.play("update");
		void invoke("agent_notify", {
			title: `Atlas ${v} is available`,
			body: "Open Atlas to see what's new and update.",
			sound: false,
		}).catch(() => {});
	}

	/**
	 * Notes the sheet shows: installed→latest while an update is on offer, the version that
	 * just got installed after an update, otherwise the release this app runs.
	 */
	async loadNotes() {
		const running = this.info?.app.version;
		const to = this.updatedTo ?? (this.available ? this.release?.version : running);
		if (!running || !to) return;
		const from = this.updatedTo || !this.available ? to : running;
		this.notesLoading = true;
		this.notesError = null;
		try {
			const all = await invoke<ReleaseNotes[]>("releases_between", { from, to });
			this.notes = selectNotes(all, from, to);
		} catch (e) {
			this.notesError = errText(e);
		} finally {
			this.notesLoading = false;
		}
	}

	/** One line about release `version` for the "Updated to" card; "" when unavailable (offline). */
	summaryOf = async (version: string): Promise<string> => {
		const cached = this.notes.find((n) => n.version === version);
		if (cached) return blurbOf(parseNotes(cached.notes));
		try {
			const all = await invoke<ReleaseNotes[]>("releases_between", { from: version, to: version });
			const n = selectNotes(all, version, version)[0];
			return n ? blurbOf(parseNotes(n.notes)) : "";
		} catch {
			return "";
		}
	};

	/** Open the sheet on the available update, the version just installed, or the running one. */
	showWhatsNew = () => {
		this.whatsNewOpen = true;
		void this.loadNotes();
	};

	/** Clears the "Updated to X" state (card dismissed or sheet closed). */
	dismissUpdated = () => (this.updatedTo = null);

	skipVersion = async () => {
		const v = this.release?.version;
		if (!v) return;
		await this.#save({ skipped: v });
		this.whatsNewOpen = false;
		toast.info(`Skipped ${v}. Atlas will tell you about the next one.`);
	};

	/** Take back "Skip this version": the pill and the notification return. */
	unskip = () => this.#save({ skipped: null, notified: null });

	setAuto = (auto: boolean) => this.#save({ auto });

	/** Explicit check (palette, Settings): always says what it found. */
	checkNow = async () => {
		await this.check();
		if (this.error) toast.error(this.error);
		else if (this.dev) toast.info("Dev build: updates are paused");
		else if (!this.available) toast.success("Atlas is up to date");
		else if (this.pill) this.showWhatsNew();
		else toast.info(`Atlas ${this.release?.version} is available (skipped)`);
	};

	async #run(kind: Run, cmd: string, args?: Record<string, unknown>) {
		this.running = kind;
		this.outcome = "running";
		this.failure = null;
		this.log = [];
		this.progress = { ...EMPTY_PROGRESS };
		this.#structured = false;
		try {
			const res = await invoke<boolean | null>(cmd, args);
			// Progress events and the command result travel on separate channels: let the last
			// `@@atlas` lines land before deciding how the run ended.
			const { promise: landed, resolve } = Promise.withResolvers<void>();
			setTimeout(resolve, 0);
			await landed;
			const ended = finishOutcome(this.progress);
			this.outcome = ended;
			if (ended === "done" && res === true) this.relaunchNeeded = true;
			// The sheet has no copy for a managed install, so its toast is shown either way.
			if (ended === "managed" || !this.whatsNewOpen) {
				const t = finishToast(kind, ended, this.progress.managed);
				toast[t.level](t.text);
			}
			await this.check();
			// The app bundle itself may have been replaced even when we could not detect it.
			if (kind === "reinstall") this.relaunchNeeded = true;
		} catch (e) {
			this.outcome = "failed";
			this.failure = errText(e);
			// The sheet shows the failure inline with Retry; elsewhere a toast is all there is.
			if (!this.whatsNewOpen) toast.error(this.failure, 10_000);
		} finally {
			this.running = null;
		}
	}

	updateAll = () => this.#run("update", "update_all");
	/** Explicit "Install X now (downgrade)": the CLI is told `--downgrade --yes`; the channel stays as persisted. */
	downgrade = () => this.#run("update", "update_all", { downgrade: true });
	reinstall = () => this.#run("reinstall", "reinstall_app");
	unlink = () => this.#run("unlink", "unlink");
	relaunch = () => invoke("relaunch").catch((e) => toast.error(errText(e)));
}

export const updates = new Updates();
