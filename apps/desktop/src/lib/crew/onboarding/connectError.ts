/**
 * What a failure of "Add a crew server" means and what to do next. The Rust side classifies (`ConnectError` in
 * `crates/atlas-desktop/src/crew_ssh.rs`, fixture `__fixtures__/crew-connect-errors.json`); this turns the kind into
 * words and into the buttons that fix it. Pure: no Svelte/Tauri, bun-testable.
 */

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
	const key = ctx.keyPath ?? "of this computer";
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
						"OpenSSH Client is not installed",
						"Atlas signs in with the ssh program that comes with Windows as an optional feature. Turn it on in Settings → System → Optional features → Add a feature → OpenSSH Client, or run this in a PowerShell opened as administrator. Then check again.",
						["recheck"],
						ADD_OPENSSH_WINDOWS,
					)
				: make(
						"ssh is not installed on this computer",
						"Atlas signs in with the ssh program. Install the OpenSSH client with your package manager (for example `sudo apt install openssh-client`), then check again.",
						["recheck"],
					);
		case "key_refused":
			return ctx.keyAdded
				? make(
						`The server still refuses the key ${key}`,
						"Atlas added it to the server, but sshd does not accept it. On the server check that PubkeyAuthentication is on and that ~/.ssh and your home folder are not writable by others.",
						["retry"],
					)
				: make(
						`The server refused the key ${key}`,
						"It is not authorized on that server yet. Sign in with the account password once and Atlas adds this key for you, or choose another key. A key with a passphrase has to be loaded in ssh-agent first.",
						["use_password", "pick_key"],
					);
		case "key_unusable":
			return make(
				`ssh cannot use the key ${key}`,
				ctx.windows
					? "Windows says other users can read it, or it is not a private key. Only your own account may have access to the file (Properties → Security → Advanced → disable inheritance). Or choose another key."
					: "Its permissions are too open (run `chmod 600` on it), or it is not a private key. Or choose another key.",
				["pick_key", "use_password"],
			);
		case "no_key":
			return make(
				"That key file is not there",
				"Choose a private key that exists, or sign in with the password and Atlas creates a key for you.",
				["pick_key", "use_password"],
			);
		case "password_refused":
			return make(
				"The server did not accept that password",
				`Check the password of ${ctx.target.split("@")[0] || "that user"} on ${host} (caps lock?) and try again.`,
				["retry"],
			);
		case "password_not_allowed":
			return make(
				"This server does not accept passwords",
				"Only SSH keys can sign in there. Use the SSH key method: someone with access has to add this computer's public key to the server first.",
				["use_key"],
			);
		case "unreachable":
			return make(
				`Cannot reach ${host}`,
				"Is the server on, and is sshd running on the port above? Check the address and the port.",
				["retry"],
			);
		case "timeout":
			return make(
				`${host} did not answer`,
				"A firewall may be dropping the connection, or the address is wrong. If the server is on a private network, connect to its VPN first.",
				["retry"],
			);
		case "name_not_found":
			return make(
				`The address ${host} does not exist`,
				"Check the spelling, or use the server's IP address.",
				["retry"],
			);
		case "host_key_changed":
			return make(
				"The server's identity changed",
				`ssh refused to continue because ${host} presents a different host key than the one this computer remembers. If you reinstalled the server this is expected: forget the old one with the command below, then try again. If you did not, someone may be intercepting the connection.`,
				["retry"],
				`ssh-keygen -R ${host}`,
			);
		case "sudo_password":
			return make(
				"This user needs a password for sudo",
				"Atlas installs system services, which takes sudo. Enter the sudo password below (usually the login password); it is used for this install only.",
				["sudo"],
			);
		case "sudo_rejected":
			return make("The sudo password was not accepted", "Check it and try again.", ["sudo"]);
		default:
			return make(
				"The install stopped",
				err.hint ?? "Atlas could not finish. The details below say where it stopped.",
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
