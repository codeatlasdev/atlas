/**
 * The capture browsers of one crew server on its settings page: status (`atlas crew browsers status`) and install
 * (`atlas crew browsers install`, streamed like the server install). Both commands run `sudo -u <crew user>` on the
 * server, so they need the ADMIN ssh address the server was installed with (`ubuntu@host[:port]`), not the crew
 * service user the desktop connects as; the registry does not keep it, so the user types it here.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { errText } from "#lib/api.ts";
import { authModeForError } from "#lib/crew/onboarding/installProgress.ts";
import { parseSSHTarget } from "#lib/crew/sshTarget.ts";
import {
	canInstall as viewCanInstall,
	checkingView,
	errorView,
	viewOf,
	type BrowsersStatus,
	type BrowsersView,
} from "./browsers.ts";

type InstallLineEvent = { line: string; is_err: boolean };

/** Lines of the install log kept on screen (the download prints a lot). */
const LOG_LINES = 200;

export class CrewBrowsers {
	/** The admin ssh address, `user@host` or `user@host:port`. */
	address = $state("");
	/** `null` until the user asked for a check: the page never SSHes into a server by itself. */
	view = $state<BrowsersView | null>(null);
	installing = $state(false);
	log = $state<string[]>([]);
	installError = $state("");
	/** The install failed on sudo: ask for the password and retry. */
	needsPassword = $state(false);
	password = $state("");

	readonly parsed = $derived(parseSSHTarget(this.address));
	readonly busy = $derived(this.view?.phase === "checking" || this.installing);
	readonly canCheck = $derived(this.parsed.ok && !this.busy);
	readonly canInstall = $derived(
		this.parsed.ok && !this.busy && this.view !== null && viewCanInstall(this.view),
	);

	private args(extra: Record<string, unknown> = {}) {
		const p = this.parsed;
		if (!p.ok) throw new Error(p.error);
		return { target: p.target, port: p.port !== 22 ? p.port : null, ...extra };
	}

	/** Ask the server what is missing. */
	async check() {
		if (!this.canCheck) return;
		this.view = checkingView;
		this.installError = "";
		try {
			const status = await invoke<BrowsersStatus>("crew_browsers_status", this.args(this.sudoArgs()));
			this.view = viewOf(status);
			this.needsPassword = false;
			this.password = "";
		} catch (e) {
			const err = errText(e);
			this.needsPassword = authModeForError(err) === "sudo";
			this.view = errorView(err);
		}
	}

	/** Install what is missing, streaming the CLI's output into `log`. */
	async install() {
		if (!this.canInstall) return;
		this.installing = true;
		this.installError = "";
		this.log = [];
		const unlisten = await listen<InstallLineEvent>("crew://install-line", (e) => {
			const line = e.payload.line.replace(/^step: \w+$/, "").trimEnd();
			if (line) this.log = [...this.log, line].slice(-LOG_LINES);
		});
		try {
			await invoke("crew_browsers_install", this.args(this.sudoArgs()));
			this.password = "";
			this.needsPassword = false;
		} catch (e) {
			const err = errText(e);
			this.installError = err;
			this.needsPassword = authModeForError(err) === "sudo";
		} finally {
			unlisten();
			this.installing = false;
		}
		if (!this.installError) await this.check();
	}

	/** The password is sent for one call and cleared by the caller; it is never kept past a successful call. */
	private sudoArgs(): Record<string, unknown> {
		return this.needsPassword && this.password ? { sudoPassword: this.password } : {};
	}
}
