import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { toast } from "@codeatlasdev/ui";
import { api, errText } from "#lib/api.ts";
import { resolveSection, type AppearanceTab, type SettingsSection } from "#lib/settings/index.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import {
	CHECK_EVERY_MS,
	EMPTY_PREFS,
	justUpdated,
	parsePrefs,
	selectNotes,
	shouldNotify,
	shouldRecheck,
	showPill,
	stepsFromLog,
	type ReleaseNotes,
	type UpdatePrefs,
} from "#lib/updates.ts";

export interface SystemInfo {
	app: { version: string; bundle: string | null; linked_to: string | null; dev: boolean };
	cli: { path: string | null; version: string | null; linked_to: string | null; error: string | null };
}

export interface Release {
	version: string;
	url: string;
	published_at: string;
	notes: string;
	/** Computed in Rust (semver): installed CLI / this app is older than `version`. */
	cli_behind: boolean;
	app_behind: boolean;
}

export type { SettingsSection, AppearanceTab } from "#lib/settings/index.ts";

type Run = "update" | "reinstall" | "unlink";

class Updates {
	info = $state<SystemInfo | null>(null);
	release = $state<Release | null>(null);
	error = $state<string | null>(null);
	checking = $state(false);
	running = $state<null | Run>(null);
	/** How the last install ended; drives the step list. */
	outcome = $state<"running" | "done" | "failed">("running");
	log = $state<string[]>([]);
	relaunchNeeded = $state(false);
	open = $state(false);
	section = $state<SettingsSection>("general");
	/** Tab inside Appearance. */
	tab = $state<AppearanceTab>("appearance");
	/** Search box text; lives here so it survives reopening and so `show` can clear it. */
	query = $state("");
	checkedAt = $state<number | null>(null);
	prefs = $state<UpdatePrefs>({ ...EMPTY_PREFS });

	/** "What's new" sheet. */
	whatsNewOpen = $state(false);
	notes = $state<ReleaseNotes[]>([]);
	notesLoading = $state(false);
	notesError = $state<string | null>(null);
	/** "Updated to X" card shown once after an update. */
	updatedTo = $state<string | null>(null);

	/**
	 * Open Settings on a section. Accepts current ids, the pre-reorganisation ids
	 * (`updates`, `shell`, `editor`) and `appearance/<tab>`.
	 */
	show = (target: string = this.section) => {
		const t = resolveSection(target);
		this.section = t.section;
		if (t.tab) this.tab = t.tab;
		this.query = "";
		this.open = true;
		if (t.section === "general") this.check();
	};

	/** Dev builds and `atlas link` must not be overwritten by a release. */
	linked = $derived(!!(this.info?.cli.linked_to || this.info?.app.linked_to));
	/** A debug/dev app (cargo tauri dev) or a linked checkout. */
	dev = $derived(this.linked || !!this.info?.app.dev);
	cliBehind = $derived(this.release?.cli_behind ?? false);
	appBehind = $derived(this.release?.app_behind ?? false);
	available = $derived(this.cliBehind || this.appBehind);
	/** The pill in the sidebar: available, not skipped, not a dev build. */
	pill = $derived(showPill(this.available, this.dev, this.prefs, this.release?.version));
	steps = $derived(stepsFromLog(this.log, this.outcome));

	#started = false;
	#unlisten: UnlistenFn[] = [];
	#timer: number | null = null;

	async init() {
		if (this.#started) return;
		this.#started = true;
		this.prefs = parsePrefs((await api.settings()).updates);
		this.#unlisten.push(
			await listen<string>("updates://log", (e) => {
				this.log.push(e.payload);
				if (this.log.length > 500) this.log.splice(0, this.log.length - 500);
			}),
			await getCurrentWindow().onFocusChanged(({ payload }) => {
				if (payload && shouldRecheck(this.prefs, this.checkedAt, Date.now())) void this.check();
			}),
		);
		this.#timer = setInterval(() => {
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
			this.release = await invoke<Release>("latest_release");
			this.checkedAt = Date.now();
			await this.#notify();
		} catch (e) {
			this.error = errText(e);
		} finally {
			this.checking = false;
		}
	}

	/** Native notification once per version, through the same path (and switch) as agent alerts. */
	async #notify() {
		const v = this.release?.version;
		if (!v || !this.pill) return;
		if (!shouldNotify(this.prefs, v, { notificationsOn: agents.notify.enabled, linked: this.dev })) return;
		await this.#save({ notified: v });
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

	async #run(kind: Run, cmd: string) {
		this.running = kind;
		this.outcome = "running";
		this.log = [];
		try {
			const res = await invoke<boolean | null>(cmd);
			if (res === true) this.relaunchNeeded = true;
			this.outcome = "done";
			toast.success(kind === "unlink" ? "Back on the release build" : "Atlas is up to date");
			await this.check();
			// The app bundle itself may have been replaced even when we could not detect it.
			if (kind === "reinstall") this.relaunchNeeded = true;
		} catch (e) {
			this.outcome = "failed";
			toast.error(errText(e), 10_000);
		} finally {
			this.running = null;
		}
	}

	updateAll = () => this.#run("update", "update_all");
	reinstall = () => this.#run("reinstall", "reinstall_app");
	unlink = () => this.#run("unlink", "unlink");
	relaunch = () => invoke("relaunch").catch((e) => toast.error(errText(e)));
}

export const updates = new Updates();
