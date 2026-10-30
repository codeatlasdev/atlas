/**
 * GitHub crew integration state.
 * Each ship logs in to GitHub itself via `gh auth login`.
 * The Mac never holds a GitHub token.
 */
import { hostCall } from "#lib/host.ts";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";

/** Status of GitHub on one ship. */
export interface ShipGithubStatus {
	installed: boolean;
	login: string | null;
}

/** Active device-flow login for one ship. */
export interface ShipLoginFlow {
	user_code: string;
	verification_uri: string;
}

class GitHubCrewStore {
	// ── per-ship status (keyed by host_key) ──────────────────────────────────
	private _statuses = $state(new Map<string, ShipGithubStatus>());
	// ── active login flows ────────────────────────────────────────────────────
	private _flows = $state(new Map<string, ShipLoginFlow>());
	private _polling = $state(new Set<string>());
	private _pollTimers = new Map<string, number>();

	// ── UI signals ────────────────────────────────────────────────────────────
	openIntegrations = $state(false);

	// ── derived ───────────────────────────────────────────────────────────────
	get statuses(): ReadonlyMap<string, ShipGithubStatus> {
		return this._statuses;
	}
	get flows(): ReadonlyMap<string, ShipLoginFlow> {
		return this._flows;
	}
	isPolling(hostKey: string): boolean {
		return this._polling.has(hostKey);
	}
	/** True when at least one ship has a GitHub login. */
	get connected(): boolean {
		for (const s of this._statuses.values()) {
			if (s.login) return true;
		}
		return false;
	}

	// ── lifecycle ─────────────────────────────────────────────────────────────
	async loadShip(hostKey: string): Promise<void> {
		try {
			const data = await hostCall<ShipGithubStatus>("bot_github_status", { host: hostKey });
			const next = new Map(this._statuses);
			next.set(hostKey, data);
			this._statuses = next;
		} catch {
			// ship unreachable — leave previous status
		}
	}

	// ── connect ───────────────────────────────────────────────────────────────
	async startLogin(hostKey: string): Promise<void> {
		try {
			const flow = await hostCall<ShipLoginFlow>("bot_github_login_start", { host: hostKey });
			const next = new Map(this._flows);
			next.set(hostKey, flow);
			this._flows = next;
			this._startPolling(hostKey);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	private _startPolling(hostKey: string): void {
		this._stopPolling(hostKey);
		const nextPoll = new Set(this._polling);
		nextPoll.add(hostKey);
		this._polling = nextPoll;
		const timer = window.setInterval(() => void this._poll(hostKey), 4000);
		this._pollTimers.set(hostKey, timer);
	}

	private async _poll(hostKey: string): Promise<void> {
		try {
			const r = await hostCall<{ state: string; login?: string; error?: string }>("bot_github_login_poll", {
				host: hostKey,
			});
			if (r.state === "done") {
				this._stopPolling(hostKey);
				const next = new Map(this._flows);
				next.delete(hostKey);
				this._flows = next;
				await this.loadShip(hostKey);
			} else if (r.state === "error") {
				this._stopPolling(hostKey);
				const next = new Map(this._flows);
				next.delete(hostKey);
				this._flows = next;
				toast.error(r.error ?? "GitHub sign-in failed. Try again.");
			}
			// "pending": keep polling
		} catch (e) {
			this._stopPolling(hostKey);
			toast.error(errText(e));
		}
	}

	stopPolling(hostKey: string): void {
		this._stopPolling(hostKey);
		const nextFlows = new Map(this._flows);
		nextFlows.delete(hostKey);
		this._flows = nextFlows;
	}

	private _stopPolling(hostKey: string): void {
		const timer = this._pollTimers.get(hostKey);
		if (timer !== undefined) {
			clearInterval(timer);
			this._pollTimers.delete(hostKey);
		}
		const next = new Set(this._polling);
		next.delete(hostKey);
		this._polling = next;
	}

	// ── disconnect ────────────────────────────────────────────────────────────
	async logout(hostKey: string): Promise<void> {
		try {
			await hostCall("bot_github_logout", { host: hostKey });
			const next = new Map(this._statuses);
			next.set(hostKey, { installed: true, login: null });
			this._statuses = next;
		} catch (e) {
			toast.error(errText(e));
		}
	}
}

export const githubCrew = new GitHubCrewStore();
