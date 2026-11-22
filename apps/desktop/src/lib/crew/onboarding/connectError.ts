/**
 * What a failure of "Add a crew server" means and what to do next. The Rust side classifies (`ConnectError` in
 * `crates/atlas-desktop/src/crew_ssh.rs`, fixture `__fixtures__/crew-connect-errors.json`); this turns the kind into
 * words and into the buttons that fix it. Pure: no Svelte/Tauri, bun-testable.
 */
import { m } from "#lib/i18n/m.ts";

/** Rust `ConnectErrorKind`, snake_case as serialized. */
export type ConnectErrorKind =
	| "ssh_missing"
	| "key_refused"
	| "key_unusable"
	| "no_key"
	| "password_refused"
	| "password_not_allowed"
	| "unreachable"
	| "timeout"
	| "name_not_found"
	| "host_key_changed"
	| "sudo_password"
	| "sudo_rejected"
	| "other";

/** Rust `ConnectError`, field for field. */
export type ConnectError = {
	kind: ConnectErrorKind;
	/** The last line the tool printed, as it printed it. */
	detail: string;
	/** A line the CLI wrote for the person (`hint: …`), when it knows the fix. */
	hint?: string | null;
};

/** Rust `KeyLookup`: what `ssh_key_resolve` answers. */
export type KeyLookup = {
	ssh_found: boolean;
	key: { identity: string; has_public: boolean } | null;
};

/** The buttons a failure offers. */
export type ConnectAction =
	| "pick_key" // choose another private key file
	| "use_password" // switch to signing in with the account password
	| "use_key" // switch to signing in with an SSH key
	| "retry"
	| "recheck" // look for ssh / the key again (after installing OpenSSH)
	| "sudo"; // type the sudo password below

export type ConnectFailure = {
	kind: ConnectErrorKind;
	title: string;
	body: string;
	/** A command to copy, when the fix is one. */
	command?: string;
	actions: ConnectAction[];
	/** The raw last line, for the "Details" disclosure. */
	detail: string;
};

export type ConnectContext = {
	/** The private key file the form shows (picked or found), or null when there is none. */
	keyPath: string | null;
	/** This run already added the key with the password, so a refused key is not "not authorized yet". */
	keyAdded: boolean;
	/** The address typed, `user@host`. */
	target: string;
	windows: boolean;
};

const ADD_OPENSSH_WINDOWS = "Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0";

/** Whatever a rejected `invoke` carried: the Rust object, a plain string, or an `Error`. */
export function parseConnectError(e: unknown): ConnectError {
	if (e && typeof e === "object" && "kind" in e && "detail" in e) {
		const o = e as { kind: ConnectErrorKind; detail: string; hint?: string | null };
		return { kind: o.kind, detail: String(o.detail), hint: o.hint ?? null };
	}
	const detail = typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
	return { kind: "other", detail, hint: null };
}

/** The words and the next steps for `err`. */
export function describeConnectError(err: ConnectError, ctx: ConnectContext): ConnectFailure {
	const host = ctx.target.slice(ctx.target.indexOf("@") + 1) || ctx.target;
	const user = ctx.target.split("@")[0];
	const make = (title: string, body: string, actions: ConnectAction[], command?: string): ConnectFailure => ({
		kind: err.kind,
		title,
		body,
		actions,
		command,
		detail: err.detail,
	});

	switch (err.kind) {
		case "ssh_missing":
			return ctx.windows
				? make(
						m.crew_onboarding_connect_error_ssh_missing_windows_title(),
						m.crew_onboarding_connect_error_ssh_missing_windows_body(),
						["recheck"],
						ADD_OPENSSH_WINDOWS,
					)
				: make(
						m.crew_onboarding_connect_error_ssh_missing_title(),
						m.crew_onboarding_connect_error_ssh_missing_body(),
						["recheck"],
					);
		case "key_refused":
			return ctx.keyAdded
				? make(
						ctx.keyPath
							? m.crew_onboarding_connect_error_key_still_refused_title({ key: ctx.keyPath })
							: m.crew_onboarding_connect_error_key_still_refused_title_default(),
						m.crew_onboarding_connect_error_key_still_refused_body(),
						["retry"],
					)
				: make(
						ctx.keyPath
							? m.crew_onboarding_connect_error_key_refused_title({ key: ctx.keyPath })
							: m.crew_onboarding_connect_error_key_refused_title_default(),
						m.crew_onboarding_connect_error_key_refused_body(),
						["use_password", "pick_key"],
					);
		case "key_unusable":
			return make(
				ctx.keyPath
					? m.crew_onboarding_connect_error_key_unusable_title({ key: ctx.keyPath })
					: m.crew_onboarding_connect_error_key_unusable_title_default(),
				ctx.windows
					? m.crew_onboarding_connect_error_key_unusable_windows_body()
					: m.crew_onboarding_connect_error_key_unusable_body(),
				["pick_key", "use_password"],
			);
		case "no_key":
			return make(
				m.crew_onboarding_connect_error_no_key_title(),
				m.crew_onboarding_connect_error_no_key_body(),
				["pick_key", "use_password"],
			);
		case "password_refused":
			return make(
				m.crew_onboarding_connect_error_password_refused_title(),
				user
					? m.crew_onboarding_connect_error_password_refused_body({ user, host })
					: m.crew_onboarding_connect_error_password_refused_body_default({ host }),
				["retry"],
			);
		case "password_not_allowed":
			return make(
				m.crew_onboarding_connect_error_password_not_allowed_title(),
				m.crew_onboarding_connect_error_password_not_allowed_body(),
				["use_key"],
			);
		case "unreachable":
			return make(
				m.crew_onboarding_connect_error_unreachable_title({ host }),
				m.crew_onboarding_connect_error_unreachable_body(),
				["retry"],
			);
		case "timeout":
			return make(
				m.crew_onboarding_connect_error_timeout_title({ host }),
				m.crew_onboarding_connect_error_timeout_body(),
				["retry"],
			);
		case "name_not_found":
			return make(
				m.crew_onboarding_connect_error_name_not_found_title({ host }),
				m.crew_onboarding_connect_error_name_not_found_body(),
				["retry"],
			);
		case "host_key_changed":
			return make(
				m.crew_onboarding_connect_error_host_key_changed_title(),
				m.crew_onboarding_connect_error_host_key_changed_body({ host }),
				["retry"],
				`ssh-keygen -R ${host}`,
			);
		case "sudo_password":
			return make(
				m.crew_onboarding_connect_error_sudo_password_title(),
				m.crew_onboarding_connect_error_sudo_password_body(),
				["sudo"],
			);
		case "sudo_rejected":
			return make(
				m.crew_onboarding_connect_error_sudo_rejected_title(),
				m.crew_onboarding_connect_error_sudo_rejected_body(),
				["sudo"],
			);
		default:
			return make(
				m.crew_onboarding_connect_error_other_title(),
				err.hint ?? m.crew_onboarding_connect_error_other_body(),
				["retry"],
			);
	}
}

/** The ssh failure kinds that a different key or the password can fix. */
export const KEY_PROBLEMS: ConnectErrorKind[] = ["key_refused", "key_unusable", "no_key"];

/**
 * A private key path from what the file picker returned: someone who picks `id_ed25519.pub` means the key next to it,
 * and ssh's `-i` wants the private file (it finds the `.pub` itself).
 */
export function privateKeyPath(picked: string): string {
	return picked.replace(/\.pub$/i, "");
}

/** `~/.ssh` of the folder holding `keyPath`, for the picker to open in: the key's own folder. */
export function folderOf(path: string): string | undefined {
	const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
	return i > 0 ? path.slice(0, i) : undefined;
}
