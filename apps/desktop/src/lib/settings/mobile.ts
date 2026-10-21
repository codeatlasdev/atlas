/** Atlas Mobile pairing: the QR payload the phone scans and the device scopes. Pure: no IPC. */

/** `mobile_pair_start` reply from hostd (`atlas_host::mobile::Mobile::pair_start`). */
export interface PairStart {
	node_id: string;
	code: string;
	expires_in_s: number;
	addrs: string[];
	relay_url: string | null;
	hostname: string;
}

export type Scope = "read" | "act" | "approve" | "term_read" | "term_write";

export interface Device {
	node_id: string;
	name: string;
	paired_at: number;
	scopes: Scope[];
	revoked: boolean;
}

export const SCOPES: { id: Scope; label: string }[] = [
	{ id: "read", label: "See projects, logs, agents" },
	{ id: "act", label: "Restart services, run actions, send prompts" },
	{ id: "approve", label: "Answer agent permission prompts" },
	{ id: "term_read", label: "Watch terminals" },
	{ id: "term_write", label: "Type in terminals" },
];

/**
 * `atlas-mobile://pair?node=<id>&code=<code>&name=<host>[&addr=ip:port…][&relay=url]`. The code is not the
 * secret that protects the link (the iroh keys are); it proves the phone was in front of this screen.
 */
export function pairUrl(p: PairStart): string {
	const q = new URLSearchParams({ node: p.node_id, code: p.code, name: p.hostname });
	for (const a of p.addrs) q.append("addr", a);
	if (p.relay_url) q.set("relay", p.relay_url);
	return `atlas-mobile://pair?${q.toString()}`;
}

/** Seconds left on a code issued at `issuedMs`, never negative. */
export function secondsLeft(p: PairStart, issuedMs: number, nowMs: number): number {
	return Math.max(0, Math.ceil(p.expires_in_s - (nowMs - issuedMs) / 1000));
}

export function toggleScope(scopes: Scope[], s: Scope, on: boolean): Scope[] {
	const rest = scopes.filter((x) => x !== s);
	return on ? SCOPES.map((x) => x.id).filter((x) => x === s || rest.includes(x)) : rest;
}
