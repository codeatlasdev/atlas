/**
 * Creating and watching a crew server's GitHub App from Settings (#643). One instance per crew server, made by the
 * card and `destroy()`ed with it.
 *
 * `create()` is the whole manifest flow:
 *
 *   crew_github_app_manifest (hostd) → github_app_listen_start (this machine: a one-shot 127.0.0.1 listener)
 *   → open its form page in the browser (it POSTs the manifest to GitHub) → github_app_listen_wait (resolves with
 *   GitHub's one-time code, or on the listener's 10 minutes) → crew_github_app_convert (hostd spends the code and keeps
 *   the key) → open GitHub's install page and watch `crew_github_app_status` until the installations change.
 *
 * The code never leaves this module except into `crew_github_app_convert`. Failures coded by hostd
 * (`crew.github_app.*`) arrive as `HostError`s already in the app's language (`hostCall`), so `error` shows them as is.
 */
import { invoke } from "@tauri-apps/api/core";
import { api, errText } from "#lib/api.ts";
import { hostCall } from "#lib/host.ts";
import { m } from "#lib/i18n/m.ts";
import {
	type GithubApp,
	type OwnerKind,
	installationsSignature,
	installUrl,
	manifestArgs,
	parseManifest,
	parseStatus,
	publicUrl,
	secondsUntil,
} from "./githubApp.ts";

/** How often the installations are asked for while GitHub's install page is open. */
const WATCH_EVERY_MS = 4000;
/** The install page is watched for at most this long; Refresh asks again whenever the user wants. */
const WATCH_FOR_MS = 10 * 60 * 1000;

type Outcome = { status: "code"; code: string } | { status: "timed_out" } | { status: "cancelled" };
interface Started {
	id: string;
	url: string;
	expires_in_secs: number;
}

/** What the card needs from the outside; the defaults are the real ones, a test swaps the clock and the timer. */
export interface Deps {
	now: () => number;
	every: (fn: () => void, ms: number) => () => void;
	openUrl: (url: string) => Promise<void>;
}

const realDeps: Deps = {
	now: () => Date.now(),
	every: (fn, ms) => {
		const t = setInterval(fn, ms);
		return () => clearInterval(t);
	},
	openUrl: (url) => api.openUrl(url),
};

export type Phase =
	/** Asking the crew server whether an app exists. */
	| "loading"
	/** No app: the form is shown. */
	| "none"
	/** Preparing the listener and the browser page. */
	| "preparing"
	/** The browser has the form; waiting for GitHub's answer (`deadline`). */
	| "waiting"
	/** The crew server is exchanging the code. */
	| "converting"
	/** An app is set up. */
	| "ready";

export class CrewGithubApp {
	phase = $state<Phase>("loading");
	app = $state<GithubApp | null>(null);
	/** The sentence the card shows in red; empty when there is none. */
	error = $state("");
	/** Ms since the epoch the listener gives up; 0 outside `waiting`. */
	deadline = $state(0);
	/** The install page is open and the installations are being watched. */
	watching = $state(false);
	refreshing = $state(false);

	owner = $state<OwnerKind>("user");
	org = $state("");
	workflows = $state(false);

	/** The webhook's public address as typed; follows the server's until the user edits it. */
	publicUrl = $state("");
	urlError = $state("");
	savingUrl = $state(false);
	/** The address the server last reported: the field follows it while it still shows it. */
	private savedUrl = "";

	/** The form's choices as hostd takes them; `null` while they are not valid (an organization without a login). */
	readonly args = $derived(manifestArgs(this.owner, this.org, this.workflows));
	readonly busy = $derived(
		this.phase === "preparing" || this.phase === "waiting" || this.phase === "converting",
	);

	private listener: string | null = null;
	/** The listener's window in minutes, for the "did not answer" sentence. */
	private windowMinutes = 10;
	private stopWatch: (() => void) | null = null;
	private gone = false;
	private polling = false;
	private readonly deps: Deps;

	constructor(
		private readonly host: string,
		deps: Partial<Deps> = {},
	) {
		this.deps = { ...realDeps, ...deps };
	}

	/** Seconds left of the listener's window; 0 outside `waiting`. */
	secondsLeft(now: number): number {
		return this.phase === "waiting" ? secondsUntil(this.deadline, now) : 0;
	}

	/** Ask the crew server for the app (first load, Refresh, after a change). */
	async load(): Promise<void> {
		try {
			this.app = parseStatus(await hostCall("crew_github_app_status", { host: this.host }));
			const url = this.app?.webhook.url ?? "";
			if (this.publicUrl === this.savedUrl) this.publicUrl = url;
			this.savedUrl = url;
			if (this.phase === "loading" || this.phase === "none" || this.phase === "ready") {
				this.phase = this.app ? "ready" : "none";
			}
			if (!this.app) this.stopWatching();
		} catch (e) {
			this.error = m.crew_github_app_load_failed({ detail: errText(e) });
			if (this.phase === "loading") this.phase = "none";
		}
	}

	async refresh(): Promise<void> {
		if (this.refreshing) return;
		this.refreshing = true;
		this.error = "";
		try {
			await this.load();
		} finally {
			this.refreshing = false;
		}
	}

	/** The whole creation flow. A second call while one runs is ignored. */
	async create(): Promise<void> {
		const args = this.args;
		if (this.phase !== "none" || !args) return;
		this.error = "";
		this.phase = "preparing";
		try {
			const reply = parseManifest(await hostCall("crew_github_app_manifest", { host: this.host, ...args }));
			if (!reply) throw new Error(m.crew_github_app_load_failed({ detail: "manifest" }));
			const started = await invoke<Started>("github_app_listen_start", {
				manifest: reply.manifest,
				postUrl: reply.postUrl,
			});
			this.listener = started.id;
			this.windowMinutes = Math.round(started.expires_in_secs / 60);
			if (this.gone) return this.closeListener();
			await this.deps.openUrl(started.url);
			this.deadline = this.deps.now() + started.expires_in_secs * 1000;
			this.phase = "waiting";
			const outcome = await invoke<Outcome>("github_app_listen_wait", { id: started.id });
			this.listener = null;
			if (this.gone) return;
			await this.finish(outcome);
		} catch (e) {
			this.closeListener();
			this.fail(e);
		}
	}

	private async finish(outcome: Outcome): Promise<void> {
		if (outcome.status === "code") {
			this.phase = "converting";
			try {
				this.app = parseStatus(
					await hostCall("crew_github_app_convert", { host: this.host, code: outcome.code }),
				);
			} catch (e) {
				this.fail(e);
				return;
			}
			this.deadline = 0;
			this.phase = this.app ? "ready" : "none";
			if (this.app) await this.openInstall();
		} else {
			if (outcome.status === "timed_out")
				this.error = m.crew_github_app_timed_out({ minutes: this.windowMinutes });
			this.deadline = 0;
			this.phase = "none";
		}
	}

	/** Back to the form with the reason; the app may exist after all (`exists`), so ask the server again. */
	private fail(e: unknown): void {
		this.error = errText(e);
		this.deadline = 0;
		this.phase = "none";
		void this.load();
	}

	/** Close the browser wait (Cancel); a no-op when nothing waits. */
	cancel(): void {
		this.closeListener();
		if (this.phase === "waiting" || this.phase === "preparing") {
			this.deadline = 0;
			this.phase = "none";
		}
	}

	private closeListener(): void {
		const id = this.listener;
		this.listener = null;
		if (id) void invoke("github_app_listen_cancel", { id }).catch(() => {});
	}

	/** Open GitHub's install page for the app and watch the installations until they change (or ten minutes pass). */
	async openInstall(): Promise<void> {
		const app = this.app;
		if (!app) return;
		try {
			await this.deps.openUrl(installUrl(app.slug));
		} catch (e) {
			this.error = errText(e);
			return;
		}
		this.startWatching(installationsSignature(app));
	}

	private startWatching(signature: string): void {
		this.stopWatching();
		const until = this.deps.now() + WATCH_FOR_MS;
		this.watching = true;
		this.stopWatch = this.deps.every(() => void this.poll(signature, until), WATCH_EVERY_MS);
	}

	private async poll(signature: string, until: number): Promise<void> {
		if (this.polling || !this.watching) return;
		this.polling = true;
		try {
			const app = parseStatus(await hostCall("crew_github_app_status", { host: this.host }));
			if (!this.watching) return;
			this.app = app;
			if (!app) this.phase = "none";
			if (!app || installationsSignature(app) !== signature) this.stopWatching();
		} catch {
			// The crew server is busy or unreachable for a moment: keep what is shown, try again on the next tick.
		} finally {
			this.polling = false;
		}
		if (this.watching && this.deps.now() >= until) this.stopWatching();
	}

	stopWatching(): void {
		this.stopWatch?.();
		this.stopWatch = null;
		this.watching = false;
	}

	/** Delete the app's key on the crew server (the caller has asked the user). */
	async remove(): Promise<void> {
		this.error = "";
		this.stopWatching();
		try {
			await hostCall("crew_github_app_remove", { host: this.host });
		} catch (e) {
			// Already gone (`none`) is the state the user asked for.
			if ((e as { code?: string }).code !== "crew.github_app.none") this.error = errText(e);
		}
		await this.load();
	}

	/** Save the webhook's public address (`crew_settings_set {public_url}`, alone: the other crew settings stay as
	 * they are). Blank turns the webhook off. An address hostd would refuse is never sent; a GitHub that could not be
	 * told is said, and the address stays saved (hostd keeps trying). */
	async savePublicUrl(): Promise<void> {
		if (this.savingUrl) return;
		const checked = publicUrl(this.publicUrl);
		if (!checked.ok) {
			this.urlError =
				checked.reason === "https"
					? m.crew_github_app_webhook_bad_https()
					: checked.reason === "long"
						? m.crew_github_app_webhook_too_long()
						: m.crew_github_app_webhook_bad_url();
			return;
		}
		this.savingUrl = true;
		this.urlError = "";
		try {
			const r = await hostCall<{ webhook_error?: string | null }>("crew_settings_set", {
				host: this.host,
				public_url: checked.url ?? "",
			});
			this.publicUrl = checked.url ?? "";
			this.savedUrl = this.publicUrl;
			if (r?.webhook_error) this.urlError = r.webhook_error;
			await this.load();
		} catch (e) {
			this.urlError = errText(e);
		} finally {
			this.savingUrl = false;
		}
	}

	/** The card left the page: close the listener and stop watching. */
	destroy(): void {
		this.gone = true;
		this.closeListener();
		this.stopWatching();
	}
}
