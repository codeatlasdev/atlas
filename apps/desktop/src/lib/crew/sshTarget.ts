/**
 * Pure parser for SSH target strings.
 * Accepted forms:
 *   user@host          → { target: "user@host",    port: 22 }
 *   user@host:2222     → { target: "user@host",    port: 2222 }
 *   user@[::1]:2222    → { target: "user@[::1]",   port: 2222 }
 *   host               → error (missing user)
 *   user@host:abc      → error (invalid port)
 *   user@host:0        → error (port out of range)
 *   user@host:65536    → error (port out of range)
 *
 * No Svelte/Tauri imports — stays bun-testable in isolation.
 */

export type ParsedTarget = { target: string; port: number };
export type ParseResult = ({ ok: true } & ParsedTarget) | { ok: false; error: string };

/**
 * Parse a raw SSH address typed by the user.
 * Normalises port: strips ":22" suffix (keeps target clean for ssh -p 22).
 */
export function parseSSHTarget(raw: string): ParseResult {
	const trimmed = raw.trim();
	if (!trimmed) return { ok: false, error: "Enter an SSH address, e.g. ubuntu@my-server.com" };

	// Must have a user@ prefix
	const atIdx = trimmed.indexOf("@");
	if (atIdx <= 0) return { ok: false, error: "Include a user, e.g. ubuntu@my-server.com" };

	const user = trimmed.slice(0, atIdx);
	if (!user) return { ok: false, error: "Include a user, e.g. ubuntu@my-server.com" };

	const hostPart = trimmed.slice(atIdx + 1);
	if (!hostPart) return { ok: false, error: "Include a hostname or IP address" };

	// Detect IPv6 bracket notation: user@[::1]:port or user@[::1]
	if (hostPart.startsWith("[")) {
		const closeBracket = hostPart.indexOf("]");
		if (closeBracket < 0)
			return { ok: false, error: "IPv6 address must be closed: user@[::1] or user@[::1]:port" };
		const ipv6Host = hostPart.slice(0, closeBracket + 1); // includes brackets
		const rest = hostPart.slice(closeBracket + 1);
		if (rest === "" || rest === ":") {
			return { ok: true, target: `${user}@${ipv6Host}`, port: 22 };
		}
		if (!rest.startsWith(":")) return { ok: false, error: 'Expected ":port" after IPv6 address' };
		return parsePort(`${user}@${ipv6Host}`, rest.slice(1));
	}

	// Regular host: find last colon that looks like a port separator.
	// A host may contain colons only in IPv6 (handled above), so any colon here is a port.
	const lastColon = hostPart.lastIndexOf(":");
	if (lastColon < 0) {
		return { ok: true, target: trimmed, port: 22 };
	}
	const host = hostPart.slice(0, lastColon);
	if (!host) return { ok: false, error: "Include a hostname or IP address" };
	return parsePort(`${user}@${host}`, hostPart.slice(lastColon + 1));
}

function parsePort(target: string, portStr: string): ParseResult {
	if (!portStr) return { ok: true, target, port: 22 };
	const n = Number(portStr);
	if (!Number.isInteger(n) || n < 1 || n > 65535) {
		return { ok: false, error: `"${portStr}" is not a valid port (1–65535)` };
	}
	return { ok: true, target, port: n };
}

/** Service users a crew server's hostd can run as: new installs use `atlas-crew`; servers installed before the rename keep `atlas-bot`. */
export const CREW_USERS = ["atlas-crew", "atlas-bot"] as const;
export const DEFAULT_CREW_USER = CREW_USERS[0];

/** Whether a hostd routing key (`ssh:user@host`) points at a crew server, i.e. at one of the crew service users. */
export function isCrewServerKey(key: string): boolean {
	return CREW_USERS.some((u) => key.startsWith(`ssh:${u}@`));
}

/** A crew server's display name: its SSH target without the service user. */
export function crewServerLabel(name: string): string {
	return name.replace(/^atlas-(?:crew|bot)@/, "");
}

/**
 * The SSH target the desktop connects to after `atlas crew install`: hostd runs as the crew service user
 * (`user` is what the installer reported; `atlas-crew` when it said nothing), not as the admin user the install
 * signed in with, whose home has no hostd.
 */
export function crewServerTarget(installTarget: string, user: string = DEFAULT_CREW_USER): string {
	const at = installTarget.indexOf("@");
	return `${user}@${at < 0 ? installTarget : installTarget.slice(at + 1)}`;
}
