/**
 * The interactive engine sign-in terminal: an `ssh -t <crew server> <engine>` pty opened through hostd, its output
 * with terminal escapes stripped, and the input line written back. Plain functions over Tauri; the onboarding state
 * decides what to do with the output and when the pty exits.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { b64decode } from "#lib/b64.ts";

/** Drop ANSI escape sequences and the text a carriage return overwrote, leaving what the user would read. */
export function stripAnsi(s: string): string {
	return s
		.replace(/\x1b(?:\[[\d;]*[A-Za-z]|\][^\x07]*(?:\x07|\x1b\\))/g, "")
		.replace(/[^\n]*\r([^\n])/g, "$1");
}

/**
 * Open `ssh -t <sshTarget> <command>` in a hostd pty and attach to it. `onCreated` gets the pty id as soon as it
 * exists, before the attach, so a failed attach still leaves the terminal panel (and its error) showing.
 */
export async function openSigninPty(
	sshTarget: string,
	command: string,
	onCreated: (pty: string) => void,
): Promise<void> {
	const r = await invoke<{ pty: string }>("host_call", {
		cmd: "pty_create",
		args: { argv: ["ssh", "-t", sshTarget, command], cwd: "/" },
	});
	onCreated(r.pty);
	await invoke("host_call", { cmd: "pty_attach", args: { pty: r.pty } });
}

/** Send one line of input (plus Enter) to the pty. */
export async function writeSigninLine(pty: string, line: string): Promise<void> {
	await invoke("host_call", { cmd: "pty_write", args: { pty, data: btoa(line + "\r") } });
}

/** Stream the pty's output (ANSI stripped) to `onOutput` and call `onExit` when it ends. Returns the cleanup. */
export function watchSigninPty(
	pty: string,
	onOutput: (text: string) => void,
	onExit: () => void,
): () => void {
	let off: (() => void) | null = null;
	void listen<{ data: string; replay?: boolean }>(`pty://${pty}`, (e) => {
		onOutput(stripAnsi(b64decode(e.payload.data)));
	}).then((fn) => {
		off = fn;
	});
	const exitOff = listen<{ ev: string; pty?: string }>("host://event", (e) => {
		if (e.payload.ev === "pty_exit" && e.payload.pty === pty) onExit();
	});
	return () => {
		off?.();
		void exitOff.then((fn) => fn());
	};
}
