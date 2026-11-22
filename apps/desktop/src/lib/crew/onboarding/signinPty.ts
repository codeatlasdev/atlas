/**
 * The interactive engine sign-in terminal: an `ssh -t <crew server> <engine>` pty opened through hostd, its output
 * with terminal escapes stripped, and the input line written back. Plain functions over Tauri; the onboarding state
 * decides what to do with the output and when the pty exits. The ssh argv (the server's port, the agents' PATH) comes
 * from Rust (`crew_signin_argv`), the same helper every other ssh call to a crew server uses.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { invokeHost } from "#lib/host.ts";
import { openPtyStream } from "#lib/ptyChannel.ts";
import { ptyStream } from "#lib/ptyStream.ts";

/** Drop ANSI escape sequences and the text a carriage return overwrote, leaving what the user would read. */
export function stripAnsi(s: string): string {
	return s
		.replace(/\x1b(?:\[[\d;]*[A-Za-z]|\][^\x07]*(?:\x07|\x1b\\))/g, "")
		.replace(/[^\n]*\r([^\n])/g, "$1");
}

/** The `pty_create` request for a terminal running `argv` (the argv is Rust's: `crew_signin_argv`). */
export function signinPtyRequest(argv: string[]): { argv: string[]; cwd: string } {
	return { argv, cwd: "/" };
}

/**
 * Open the sign-in terminal of crew server `hostKey` (`ssh:user@host`) running `command` in a hostd pty and attach to
 * it. `onCreated` gets the pty id as soon as it exists, before the attach, so a failed attach still leaves the
 * terminal panel (and its error) showing.
 */
export async function openSigninPty(
	hostKey: string,
	command: string,
	onCreated: (pty: string) => void,
): Promise<void> {
	const argv = await invoke<string[]>("crew_signin_argv", { hostKey, command });
	const r = await invokeHost<{ pty: string }>("pty_create", signinPtyRequest(argv));
	onCreated(r.pty);
	// Title/exit events only: the output is read from the stream `watchSigninPty` opens.
	await invokeHost("pty_attach", { pty: r.pty, output: false });
}

/** Send one line of input (plus Enter) to the pty. */
export async function writeSigninLine(pty: string, line: string): Promise<void> {
	await invokeHost("pty_write", { pty, data: btoa(line + "\r") });
}

/** Stream the pty's output (ANSI stripped) to `onOutput` and call `onExit` when it ends. Returns the cleanup. */
export function watchSigninPty(
	pty: string,
	onOutput: (text: string) => void,
	onExit: () => void,
): () => void {
	const text = new TextDecoder();
	const stream = ptyStream({
		open: (f) => openPtyStream(pty, f),
		attached: async () => {},
		write: (b, _replay, done) => {
			onOutput(stripAnsi(text.decode(b, { stream: true })));
			done();
		},
		reset: () => {},
	});
	const exitOff = listen<{ ev: string; pty?: string }>("host://event", (e) => {
		if (e.payload.ev === "pty_exit" && e.payload.pty === pty) onExit();
	});
	return () => {
		stream.dispose();
		void exitOff.then((fn) => fn());
	};
}
