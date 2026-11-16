/**
 * The local terminal host's version state (see `crates/atlas-desktop/src/host.rs`): Rust
 * reconciles the daemon with this app on every (re)connect and pushes `host://status`; this
 * store mirrors it, owns the restart prompt and wraps the three user choices.
 *
 * `hostState` is the contract other modules (editor/LSP, sidebar, settings, tray) read.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { EMPTY_HOST, parseHostState, shouldPrompt, type HostState } from "#lib/hostStatus.ts";
import { gpu } from "#lib/stores/gpu.svelte.ts";

class Host {
	status = $state<HostState["status"]>(EMPTY_HOST.status);
	version = $state<string | null>(EMPTY_HOST.version);
	proto = $state<number | null>(EMPTY_HOST.proto);
	caps = $state<string[]>(EMPTY_HOST.caps);
	appVersion = $state(EMPTY_HOST.appVersion);
	skew = $state<HostState["skew"]>(EMPTY_HOST.skew);
	/** Running terminals only. */
	ptys = $state<HostState["ptys"]>(EMPTY_HOST.ptys);
	/** The sentence from `skew_reason`; empty when current. */
	reason = $state(EMPTY_HOST.reason);
	/** The user chose "Later" this session. */
	deferred = $state(EMPTY_HOST.deferred);
	idleArmed = $state(EMPTY_HOST.idleArmed);
	missing = $state<string[]>(EMPTY_HOST.missing);

	/** The host serves language servers (`lsp_*`). */
	get lspReady() {
		return this.caps.includes("lsp");
	}

	set(s: HostState) {
		Object.assign(this, s);
	}
}

export const hostState = new Host();

/** The restart prompt dialog. */
export const hostPrompt = $state({ open: false });

function apply(raw: unknown) {
	const s = parseHostState(raw);
	if (!s) return;
	hostState.set(s);
	// Terminals fall back to xterm.js while the host lacks the raw byte stream.
	gpu.hostTooOld = s.missing.includes("pty_attach_raw");
	// The prompt has no point once the host is current or the terminals are gone.
	if (hostPrompt.open && !shouldPrompt(s)) close();
}

async function refresh() {
	apply(await invoke("host_status").catch(() => null));
}

/**
 * Rust raised the restart prompt (once per daemon, decided there). The event can be lost: the
 * daemon is usually reconciled before this webview listens. So the pending flag is read from
 * Rust (`host_take_prompt`, cleared on read) both on the event and once at startup.
 */
async function takePrompt() {
	await refresh();
	const pending = await invoke<boolean>("host_take_prompt").catch(() => false);
	if (pending && shouldPrompt(hostState)) {
		acted = false;
		hostPrompt.open = true;
	}
}

/** The user picked one of the three buttons: closing the dialog is then not a dismissal. */
let acted = false;

function close() {
	acted = hostPrompt.open;
	hostPrompt.open = false;
}

/** Esc / close button / backdrop: the same as "Later", unless a button already answered. */
export function dismissHostPrompt() {
	if (acted) {
		acted = false;
		return;
	}
	postponeHost();
}

/** "Restart now": the user agreed to end the terminals. */
export async function restartHost() {
	close();
	try {
		await invoke("host_restart");
	} catch (e) {
		toast.error(errText(e));
	}
}

/** "Later": no prompt for the rest of this session; the chip stays. */
export function postponeHost() {
	close();
	void invoke("host_postpone").catch((e) => toast.error(errText(e)));
}

/** "Restart when idle": the host restarts the moment no terminal runs. */
export function restartHostWhenIdle() {
	close();
	void invoke("host_restart_when_idle").catch((e) => toast.error(errText(e)));
}

/** Open the prompt by hand (chip, settings row, menu-bar entry). */
export function openHostPrompt() {
	acted = false;
	hostPrompt.open = true;
}

/**
 * Follow the host: the state is read once too, since the daemon is reconciled at startup,
 * possibly before these listeners exist. A finished install (`updates://progress` host event)
 * and every reconnect re-read it.
 */
export async function initHostStatus(): Promise<UnlistenFn> {
	const offs = await Promise.all([
		listen("host://status", (e) => apply(e.payload)),
		listen("host://prompt", () => void takePrompt()),
		listen<{ kind?: string; text?: string }>("host://toast", (e) => {
			if (!e.payload.text) return;
			// `warning`: events were dropped and resynced (#299); anything else is a finished upgrade.
			if (e.payload.kind === "warning") toast.info(e.payload.text);
			else toast.success(e.payload.text);
		}),
		listen("host://reconnected", () => void refresh()),
		listen<{ kind?: string }>("updates://progress", (e) => {
			if (e.payload.kind === "host") void refresh();
		}),
	]);
	await takePrompt();
	return () => offs.forEach((f) => f());
}
