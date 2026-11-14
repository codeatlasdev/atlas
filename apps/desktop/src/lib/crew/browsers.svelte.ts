/**
 * The capture browsers of one crew server on its settings page: status (`atlas crew browsers status`) and install
 * (`atlas crew browsers install`, streamed like the server install). Both commands run `sudo -u <crew user>` on the
 * server, so they need the ADMIN ssh address the server was installed with (`ubuntu@host[:port]`), not the crew
 * service user the desktop connects as. The registry keeps it (`crew-servers.json`, `admin`): the field starts from it,
 * is editable, and a change the user commits is saved back. A server that has none starts empty and gets what the
 * user typed once a check worked.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { errText } from "#lib/api.ts";
import { authModeForError } from "#lib/crew/onboarding/installProgress.ts";
import type { AdminAddress } from "#lib/crew.ts";
import { parseSSHTarget } from "#lib/crew/sshTarget.ts";
import {
	addressText,
	adminOf,
	canInstall as viewCanInstall,
	checkingView,
	errorView,
	sameAdmin,
	viewOf,
	type BrowsersStatus,
	type BrowsersView,
} from "./browsers.ts";

type InstallLineEvent = { line: string; is_err: boolean };

/** Lines of the install log kept on screen (the download prints a lot). */
const LOG_LINES = 200;

/** Where a server's admin address is kept: the registry (`crew.svelte.ts`), behind this seam so the state is testable. */
export interface AdminBook {
	/** What the registry holds now. */
	current(): AdminAddress | undefined;
	save(admin: AdminAddress): Promise<void>;
}

export class CrewBrowsers {
	/** The admin ssh address, `user@host` or `user@host:port`; starts from the registry's. */
	address = $state("");
	/** `null` until the user asked for a check: the page never SSHes into a server by itself. */
	view = $state<BrowsersView | null>(null);
	installing = $state(false);
	log = $state<string[]>([]);
	installError = $state("");
	/** The install failed on sudo: ask for the password and retry. */
	needsPassword = $state(false);
	password = $state("");

	/** Saving the address failed (the row says so; the check or install itself is unaffected). */
	saveError = $state("");

	constructor(private readonly book?: AdminBook) {
		this.address = addressText(book?.current());
	}

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

	/**
	 * Keep the address in the registry when it is valid and differs from what is kept. Called when the user commits
	 * it (leaves the field, Enter) and after a check that worked; a half-typed address never overwrites a good one.
	 */
	async saveAddress() {
		const p = this.parsed;
		if (!this.book || !p.ok) return;
		const admin = adminOf(p);
		if (sameAdmin(this.book.current(), admin)) return;
		try {
			await this.book.save(admin);
			this.saveError = "";
		} catch (e) {
			this.saveError = errText(e);
		}
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
			// The address reached the server and the CLI answered: it is worth keeping.
			await this.saveAddress();
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
