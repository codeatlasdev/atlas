/**
 * The sentences the terminal host and the Tauri shell send as a stable `code` plus `args` (and an English `message`
 * for logs and for a client that does not know the code), written in the app's language at the moment they are shown.
 *
 * Producers: `crates/atlas-host/src/client.rs` (`CODES`), `crates/atlas-host/src/upgrade.rs` (`hostd.older`),
 * `crates/atlas-desktop/src/host.rs` (`host://status`, `host://toast`, `host://error`, a rejected `host_call`) and
 * `crates/atlas-lsp/src/install.rs` (`reason` of a language server). A code added there is added to the switch below
 * in the same change: `hostText.test.ts` renders every code of the fixtures those crates write.
 *
 * Rules: an unknown code, or a known one whose args are missing or of the wrong type (a daemon from before the args
 * existed), shows the English `message`. `m.key` stays static (a switch, never `m[code]`) so messages tree-shake.
 * Pure: no Svelte, no Tauri. Call it inside a template, `$derived` or at the moment of showing, never cached.
 */
import { classify } from "#lib/errors/diagnostics.ts";
import { messageOf } from "#lib/errors/report.ts";
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";

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

function feature(cap: string): string {
	switch (cap) {
		case "lsp":
			return m.hostd_wire_feature_lsp();
		case "pty_attach_raw":
			return m.hostd_wire_feature_pty_attach_raw();
		case "agents":
			return m.hostd_wire_feature_agents();
		default:
			return cap;
	}
}

/** The catalog text of `code`, or `undefined` when the code is unknown or an argument it needs is missing. */
export function render(code: string, args: Record<string, unknown> = {}): string | undefined {
	const s = (k: string) => str(args[k]);
	switch (code) {
		case "hostd.older": {
			const version = s("version");
			const app_version = s("app_version");
			if (version === undefined || app_version === undefined) return undefined;
			const missing = Array.isArray(args.missing) ? args.missing.filter((c) => typeof c === "string") : [];
			return missing.length
				? m.hostd_wire_older_features({ version, app_version, features: list(missing.map(feature)) })
				: m.hostd_wire_older({ version, app_version });
		}
		case "hostd.unreachable": {
			const why = s("why");
			return why ? m.hostd_wire_unreachable_why({ why }) : m.hostd_wire_unreachable();
		}
		case "hostd.upgrade_failed": {
			const error = s("error");
			return error === undefined ? undefined : m.hostd_wire_upgrade_failed({ error });
		}
		case "hostd.remote_unreachable": {
			const host = s("host");
			const why = s("why");
			return host === undefined || why === undefined
				? undefined
				: m.hostd_wire_remote_unreachable({ host, why });
		}
		case "hostd.remote_connecting": {
			const host = s("host");
			return host === undefined ? undefined : m.hostd_wire_remote_connecting({ host });
		}
		case "hostd.upgraded":
			return m.hostd_wire_upgraded();
		case "hostd.events_dropped":
			return m.hostd_wire_events_dropped();
		case "hostd.closed":
			return m.hostd_wire_closed();
		case "hostd.disconnected":
			return m.hostd_wire_disconnected();
		case "hostd.start_failed":
			return m.hostd_wire_start_failed();
		case "hostd.refused": {
			const why = s("why");
			return why === undefined ? undefined : m.hostd_wire_refused({ why });
		}
		case "hostd.cli_not_found": {
			const command = s("command");
			return command === undefined ? undefined : m.hostd_wire_cli_not_found({ command });
		}
		default:
			return undefined;
	}
}

/** The text to show for `w`: the catalog's when the code is known, else the English `message`. */
export function hostText(w: Wire): string {
	return (w.code ? render(w.code, w.args) : undefined) ?? w.message ?? "";
}

/** A rejected `host_call` that carried a code: its message is already in the app's language. */
export class HostError extends Error {
	readonly code: string;
	readonly args: Record<string, unknown>;
	/** The English the shell sent with it. */
	readonly english: string;
	constructor(w: Wire & { code: string }) {
		super(hostText(w));
		this.name = "HostError";
		this.code = w.code;
		this.args = w.args ?? {};
		this.english = w.message ?? "";
	}
}

/**
 * What a `host_call` / `host_pty_list` rejection becomes: a `HostError` when the shell coded it, else the rejection
 * untouched (a string of hostd's or the OS's own text, as it always was).
 */
export function hostRejection(e: unknown): unknown {
	const w = isObject(e) && !(e instanceof Error) ? parseWire(e) : null;
	return w?.code ? new HostError({ ...w, code: w.code }) : e;
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
	if (e instanceof HostError)
		return OUTAGE.has(e.code) || (e.code === "hostd.refused" && e.args.why === "unauthorized");
	return classify(message) === "host";
}
