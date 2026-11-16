/** Atlas Mobile pairing: the QR payload the phone scans, the device scopes and how Settings words them. Pure: no IPC. */

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

/** The permissions, grouped the way they are edited, each with what granting it risks. Order = `ALL_SCOPES` in hostd. */
export const SCOPE_GROUPS: { title: string; scopes: { id: Scope; label: string; risk: string }[] }[] = [
	{
		title: "View",
		scopes: [
			{
				id: "read",
				label: "See projects, logs and agents",
				risk: "Reads names, status, logs and diffs. Changes nothing.",
			},
			{
				id: "term_read",
				label: "Watch terminals",
				risk: "Sees everything a terminal prints, secrets included.",
			},
		],
	},
	{
		title: "Control",
		scopes: [
			{
				id: "act",
				label: "Restart services, run actions, send prompts",
				risk: "Changes your running stack, commits and pushes, tells agents what to do.",
			},
			{
				id: "term_write",
				label: "Type in terminals",
				risk: "Runs commands as you, like sitting at the keyboard.",
			},
			{
				id: "approve",
				label: "Answer agent permission prompts",
				risk: "Can allow an agent to run a tool or a command.",
			},
		],
	},
];

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
	return on ? SCOPES.map((x) => x.id).filter((x) => x === s || rest.includes(x)) : rest;
}

export interface Access {
	/** `Full access`, `View only`, `No access`, `Custom: 3 of 5`. */
	label: string;
	kind: "full" | "view" | "none" | "custom";
	/** One plain sentence on what the phone can do. */
	detail: string;
}

/** What a phone's scopes add up to, in words. */
export function access(scopes: Scope[]): Access {
	const all = SCOPES.length;
	const on = SCOPES.filter((s) => scopes.includes(s.id));
	if (on.length === all) {
		return {
			label: "Full access",
			kind: "full",
			detail: "Can see this computer, run actions, type in terminals and answer agent prompts.",
		};
	}
	if (on.length === 0) {
		return {
			label: "No access",
			kind: "none",
			detail: "Paired, but it can do nothing until you allow something.",
		};
	}
	if (scopes.includes("read") && on.every((s) => s.id === "read" || s.id === "term_read")) {
		return {
			label: "View only",
			kind: "view",
			detail: scopes.includes("term_read")
				? "Can look at projects, logs, agents and terminals. Cannot change anything."
				: "Can look at projects, logs and agents. Cannot change anything or watch terminals.",
		};
	}
	return {
		label: `Custom: ${on.length} of ${all}`,
		kind: "custom",
		detail: `Can: ${on.map((s) => s.label.toLowerCase()).join("; ")}.`,
	};
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
	return isGenericName(d.name) ? `Phone ${shortId(d.node_id)}` : d.name.trim();
}

/** `just now`, `3 min ago`, `2 h ago`, `yesterday`, `5 days ago`, then the date. */
export function relativeTime(ts: number, nowMs: number, locale?: string): string {
	const s = Math.max(0, Math.floor(nowMs / 1000 - ts));
	if (s < 45) return "just now";
	const min = Math.round(s / 60);
	if (min < 60) return `${min} min ago`;
	const h = Math.round(s / 3600);
	if (h < 24) return `${h} h ago`;
	const days = Math.floor(s / 86400);
	if (days === 1) return "yesterday";
	if (days < 30) return `${days} days ago`;
	return `on ${new Date(ts * 1000).toLocaleDateString(locale)}`;
}

/** The line under a phone's name: connection, last seen, pairing date (the date follows the user's locale). */
export function deviceStatus(d: Device, nowMs: number, locale?: string): string {
	const paired = `Paired ${new Date(d.paired_at * 1000).toLocaleDateString(locale)}`;
	if (d.connected) return `Connected now · ${paired}`;
	if (d.last_seen === undefined) return `Not connected yet · ${paired}`;
	return `Last seen ${relativeTime(d.last_seen, nowMs, locale)} · ${paired}`;
}

/** Phones, the one that is connected or was seen last first. */
export function sortDevices(devices: Device[]): Device[] {
	const rank = (d: Device) => (d.connected ? Infinity : (d.last_seen ?? d.paired_at));
	return [...devices].sort((a, b) => rank(b) - rank(a) || b.paired_at - a.paired_at);
}

/** Past-tense words for `project_service` ops. */
const OPS: Record<string, string> = { restart: "Restarted", start: "Started", stop: "Stopped" };

/** `"<12 chars>"` (what `audit.log` keeps of typed text) → 12. */
function sizeOf(v: unknown): number | null {
	const m = typeof v === "string" ? /^<(\d+) (?:chars|bytes base64)>$/.exec(v) : null;
	return m ? Number(m[1]) : null;
}

const str = (v: unknown) => (typeof v === "string" && v ? v : null);

/** One line of `audit.log` as a sentence. Unknown commands show their name, never nothing. */
export function describeActivity(e: Activity): string {
	const a = e.args ?? {};
	switch (e.cmd) {
		case "pair":
			return "Paired with this computer";
		case "project_service": {
			const op = OPS[str(a.op) ?? ""] ?? `Ran ${str(a.op) ?? "an operation on"}`;
			return `${op} ${str(a.service) ?? "the stack"}`;
		}
		case "project_action":
			return `Ran the action ${str(a.key) ?? ""}`.trim();
		case "agent_send": {
			const n = sizeOf(a.text);
			return n === null ? "Sent a prompt to an agent" : `Sent a prompt to an agent (${n} characters)`;
		}
		case "agent_approve":
			return a.allow === true ? "Allowed an agent's tool call" : "Denied an agent's tool call";
		case "agent_interrupt":
			return "Interrupted an agent";
		case "agent_launch":
			return `Started ${str(a.agent) ?? "an agent"}`;
		case "pty_open":
			return "Opened a terminal";
		case "pty_write": {
			const n = sizeOf(a.data);
			return n === null ? "Typed in a terminal" : `Typed in a terminal (${Math.floor((n * 3) / 4)} bytes)`;
		}
		case "pty_signal":
			return "Sent a signal to a terminal";
		case "git_stage":
			return "Staged files";
		case "git_unstage":
			return "Unstaged files";
		case "git_commit":
			return "Committed";
		case "git_push":
			return "Pushed a branch";
		case "git_pr_create":
			return "Opened a pull request";
		case "worktree_create":
			return "Created a worktree";
		case "worktree_remove":
			return "Removed a worktree";
		case "crewmate_task":
			return "Sent a task to a crewmate";
		case "crewmate_cancel":
			return "Cancelled a crewmate's task";
		case "crewmate_answer":
			return "Answered a crewmate's question";
		default: {
			const t = e.cmd.replace(/_/g, " ").trim();
			return t ? t[0].toUpperCase() + t.slice(1) : "Unknown action";
		}
	}
}
