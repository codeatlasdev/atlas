/**
 * The sentences the terminal host and the Tauri shell send as a stable `code` plus `args` (and an English `message`
 * for logs and for a client that does not know the code), written in the app's language at the moment they are shown.
 *
 * The catalog side is `#lib/hostMessages.ts`, loaded on demand so the startup bundle does not carry it. An event
 * handler writes with `await hostTextLoaded(w)`; a reactive reader (`hostReason`) calls `hostText`, which re-runs when
 * the messages arrive and meanwhile shows nothing for a coded text (never the English `message`: it would flash in
 * another language). An unknown code, a known one whose args are missing or mistyped (a daemon from before the args
 * existed) and a messages chunk that failed to load show the English `message`. Call it inside a template,
 * `$derived` or at the moment of showing, never cached.
 */
import { classify } from "#lib/errors/diagnostics.ts";
import { messageOf } from "#lib/errors/report.ts";
import type { HostError } from "#lib/hostMessages.ts";
import { loaded, loadHostMessages } from "#lib/hostMessagesLoad.svelte.ts";

export { loadHostMessages };

/** What a coded text carries on the wire; every key is optional so an old payload (a bare string) fits. */
export interface Wire {
	code?: string;
	args?: Record<string, unknown>;
	/** English, for logs and for a code this build does not know. */
	message?: string;
}

const isObject = (v: unknown): v is Record<string, unknown> =>
	!!v && typeof v === "object" && !Array.isArray(v);
const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);

/** A payload (`{code, args, message}`, `{message}` or a bare string) as a `Wire`; `null` for anything else. */
export function parseWire(raw: unknown): Wire | null {
	if (typeof raw === "string") return { message: raw };
	if (!isObject(raw)) return null;
	return { code: str(raw.code), args: isObject(raw.args) ? raw.args : undefined, message: str(raw.message) };
}

/** The text to show for `w`: the catalog's when the code is known, else the English `message`. */
export function hostText(w: Wire): string {
	if (!w.code) return w.message ?? "";
	const messages = loaded(); // inside an effect: re-runs when the messages load
	return messages ? (messages.render(w.code, w.args) ?? w.message ?? "") : "";
}

/** `hostText` once the messages are in: what an event handler shows, so the sentence is never late nor English. */
export async function hostTextLoaded(w: Wire): Promise<string> {
	await loadHostMessages();
	return hostText(w);
}

/**
 * What a `host_call` / `host_pty_list` rejection becomes: a `HostError` when the shell coded it, else the rejection
 * untouched (a string of hostd's or the OS's own text, as it always was). A `HostError` that cannot be built (its
 * chunk failed to load) leaves the rejection untouched too.
 */
export async function hostRejection(e: unknown): Promise<unknown> {
	const w = isObject(e) && !(e instanceof Error) ? parseWire(e) : null;
	if (!w?.code) return e;
	await loadHostMessages();
	return loaded()?.hostError?.(w) ?? e;
}

/** The codes that say the connection to a terminal host is gone: the local host's outage, the one with Restart. */
const OUTAGE = new Set(["hostd.unreachable", "hostd.closed", "hostd.disconnected", "hostd.start_failed"]);

/**
 * Is `e` a failure of the terminal host itself (one shared toast with Restart and Report) and not an ordinary error?
 * A coded failure says so by its code, in any language: the connection to the host is gone (`OUTAGE`), or hostd
 * refused the token. A remote host that is still connecting or unreachable, a missing CLI and a failed upgrade are
 * the caller's own errors, as their sentences always were. A text is matched the way it always was (`classify`).
 */
export function isHostFailure(e: unknown, message = messageOf(e)): boolean {
	if (e instanceof Error && e.name === "HostError") {
		const { code, args } = e as HostError;
		return OUTAGE.has(code) || (code === "hostd.refused" && args.why === "unauthorized");
	}
	return classify(message) === "host";
}
