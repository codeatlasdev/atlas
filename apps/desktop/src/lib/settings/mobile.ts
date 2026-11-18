/**
 * Atlas Mobile pairing: the QR payload the phone scans and the device list. Pure: no IPC. The words of the Mobile settings
 * (permissions, access, activity) are in `mobileText.ts`: this module is imported at startup and must stay small.
 */
import { m } from "#lib/i18n/m.ts";

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

/** One entry of `mobile_devices` (`atlas_host::mobile::Mobile::devices_view`). */
export interface Device {
	node_id: string;
	name: string;
	/** Unix seconds. */
	paired_at: number;
	scopes: Scope[];
	revoked: boolean;
	/** A stream from the phone is open now. */
	connected: boolean;
	/** Unix seconds: when the phone last connected, disconnected or acted. Absent until it first connects. */
	last_seen?: number;
}

/** One entry of `mobile_activity`: a line of `audit.log` (typed text is only a length in `args`). */
export interface Activity {
	ts: number;
	cmd: string;
	args: Record<string, unknown> | null;
}

/**
 * Where to get Atlas Mobile. The releases go to TestFlight and Play internal testing before the stores
 * (docs/mobile.md "Releases"), so there is no public listing to link yet: until `appStore` / `playStore` hold
 * their URLs, the pane links `docs` and says the app is in testing. Add the two URLs here and nowhere else.
 */
export const MOBILE_APP: { docs: string; appStore: string | null; playStore: string | null } = {
	docs: "https://atlas.codeatlas.com.br/docs/mobile/",
	appStore: null,
	playStore: null,
};

/** docs/mobile.md "Pairing fails with a VPN or WARP on". */
export const VPN_DOCS = `${MOBILE_APP.docs}#pairing-fails-with-a-vpn-or-warp-on`;

/** Permission ids in the order hostd lists them (`ALL_SCOPES`); a set of scopes is kept in this order. */
export const SCOPE_IDS: Scope[] = ["read", "act", "approve", "term_read", "term_write"];

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
	// The shared clock ticks once a second, so its `now` can be a moment older than `issuedMs`: never above the full TTL.
	return Math.min(p.expires_in_s, Math.max(0, Math.ceil(p.expires_in_s - (nowMs - issuedMs) / 1000)));
}

/** `1:42`. */
export function clock(seconds: number): string {
	const s = Math.max(0, Math.floor(seconds));
	return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** `012345` → `012 345`. */
export function groupCode(code: string): string {
	return code.replace(/(\d{3})(?=\d)/g, "$1 ");
}

export function toggleScope(scopes: Scope[], s: Scope, on: boolean): Scope[] {
	const rest = scopes.filter((x) => x !== s);
	return on ? SCOPE_IDS.filter((x) => x === s || rest.includes(x)) : rest;
}

/** The name the phone sent when pairing is the same for every Atlas Mobile, so it names no one. */
export function isGenericName(name: string): boolean {
	return ["", "atlas mobile", "phone"].includes(name.trim().toLowerCase());
}

/** `A3F9`: enough of the phone's key to tell two unnamed phones apart. */
export function shortId(nodeId: string): string {
	return nodeId.slice(0, 4).toUpperCase();
}

/** What a phone is called in the list: the name it has, else `Phone A3F9` (rename it to say whose it is). */
export function deviceTitle(d: Pick<Device, "name" | "node_id">): string {
	return isGenericName(d.name) ? m.mobile_device_title({ id: shortId(d.node_id) }) : d.name.trim();
}

/** Phones, the one that is connected or was seen last first. */
export function sortDevices(devices: Device[]): Device[] {
	const rank = (d: Device) => (d.connected ? Infinity : (d.last_seen ?? d.paired_at));
	return [...devices].sort((a, b) => rank(b) - rank(a) || b.paired_at - a.paired_at);
}
