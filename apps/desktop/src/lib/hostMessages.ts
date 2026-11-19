/**
 * The catalog side of `#lib/hostText.ts` (and `HostError`, the error a coded rejection becomes: only a rejection that
 * carries a code needs it, and `isHostFailure` recognizes it by name without it): the sentence of each code the terminal host and the Tauri shell send
 * (`code` plus `args`). Loaded on demand (`loadHostMessages`), never at startup: these messages are only needed
 * when a host failure, a host toast or a host status reason is shown.
 *
 * Producers: `crates/atlas-host/src/client.rs` (`CODES`), `crates/atlas-host/src/protocol.rs` (`REPLY_CODES`: a failed
 * `fs_*` or `exec` request), `crates/atlas-host/src/upgrade.rs` (`hostd.older`), `crates/atlas-desktop/src/host.rs`.
 * A code added there is added to the switch below in the same change: `hostText.test.ts` and `hostReplyText.test.ts`
 * render every code of the fixtures those crates write. `m.key` stays static (a switch, never `m[code]`) so messages
 * tree-shake.
 */
import type { Wire } from "#lib/hostText.ts";
import { appLocale } from "#lib/i18n/locale.svelte.ts";
import { m } from "#lib/i18n/m.ts";

const str = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);

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
				? m.hostd_wire_older_features({
						version,
						app_version,
						features: new Intl.ListFormat(appLocale()).format(missing.map(feature)),
					})
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
		case "hostd.busy":
			return m.hostd_wire_busy();
		case "hostd.too_many_clients":
			return m.hostd_wire_too_many_clients();
		case "fs.not_found": {
			const path = s("path");
			return path === undefined ? undefined : m.hostd_wire_fs_not_found({ path });
		}
		case "fs.io": {
			const path = s("path");
			if (path === undefined) return undefined;
			switch (s("kind")) {
				case "permission_denied":
					return m.hostd_wire_fs_permission_denied({ path });
				case "not_a_directory":
					return m.hostd_wire_fs_not_a_directory({ path });
				case "is_a_directory":
					return m.hostd_wire_fs_is_a_directory({ path });
				case "already_exists":
					return m.hostd_wire_fs_already_exists({ path });
				case "other":
					return m.hostd_wire_fs_other({ path });
				default:
					return undefined;
			}
		}
		case "exec.spawn_failed": {
			const program = s("program");
			if (program === undefined) return undefined;
			switch (s("reason")) {
				case "not_found":
					return m.hostd_wire_spawn_not_found({ program });
				case "permission_denied":
					return m.hostd_wire_spawn_permission_denied({ program });
				case "other":
					return m.hostd_wire_spawn_other({ program });
				default:
					return undefined;
			}
		}
		case "hostd.invalid_argument":
			switch (s("reason")) {
				case "bad_base64": {
					const field = s("field");
					return field === undefined ? undefined : m.hostd_wire_invalid_base64({ field });
				}
				case "empty":
					return m.hostd_wire_invalid_empty();
				case "no_parent":
					return m.hostd_wire_invalid_no_parent();
				default:
					return undefined;
			}
		default:
			return undefined;
	}
}

/** A rejected `host_call` that carried a code: its message is already in the app's language. */
export class HostError extends Error {
	readonly code: string;
	readonly args: Record<string, unknown>;
	constructor(w: Wire) {
		super((w.code && render(w.code, w.args)) || w.message || "");
		this.name = "HostError";
		this.code = w.code ?? "";
		this.args = w.args ?? {};
	}
}

/** The `HostError` of a coded rejection. */
export const hostError = (w: Wire) => new HostError(w);
