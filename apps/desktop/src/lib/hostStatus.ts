/**
 * Pure side of the terminal-host status (`host://status`, see `crates/atlas-desktop/src/host.rs`):
 * defensive parsing of the payload, the sidebar chip, the restart prompt copy and the
 * prompt rule. No Svelte, no Tauri: `stores/hostStatus.svelte.ts` holds the state.
 */
import { tilde } from "#lib/format.ts";
import { hostText } from "#lib/hostText.ts";
import { m } from "#lib/i18n/m.ts";

export type HostStatus = "current" | "upgrading" | "upgraded" | "outdated" | "restarting";
export type HostSkew = "none" | "upgradable" | "needsRestart";

export interface HostPty {
	id: string;
	title: string;
	cwd: string;
	argv: string[];
}

export interface HostState {
	status: HostStatus;
	version: string | null;
	proto: number | null;
	caps: string[];
	appVersion: string;
	skew: HostSkew;
	/** Running terminals only. */
	ptys: HostPty[];
	/** The English sentence from `skew`; empty when current. Shown only for a code this build does not know. */
	reason: string;
	/** The code of that sentence (`hostd.older`) and its arguments; empty when current or from an older shell. */
	reasonCode: string;
	reasonArgs: Record<string, unknown>;
	/** The user chose "Later" this session. */
	deferred: boolean;
	/** The user chose "Restart when idle". */
	idleArmed: boolean;
	missing: string[];
}

export const EMPTY_HOST: HostState = {
	status: "current",
	version: null,
	proto: null,
	caps: [],
	appVersion: "",
	skew: "none",
	ptys: [],
	reason: "",
	reasonCode: "",
	reasonArgs: {},
	deferred: false,
	idleArmed: false,
	missing: [],
};

const STATUSES: readonly string[] = ["current", "upgrading", "upgraded", "outdated", "restarting"];
const SKEWS: readonly string[] = ["none", "upgradable", "needsRestart"];

const str = (v: unknown) => (typeof v === "string" ? v : "");
const strs = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x) => typeof x === "string") : []);

/** An object's fields as `unknown`, `null` for anything else (arrays included). */
function fields(v: unknown): Record<string, unknown> | null {
	return v && typeof v === "object" && !Array.isArray(v) ? Object.fromEntries(Object.entries(v)) : null;
}

function parsePty(raw: unknown): HostPty | null {
	const o = fields(raw);
	if (!o || typeof o.id !== "string") return null;
	return { id: o.id, title: str(o.title), cwd: str(o.cwd), argv: strs(o.argv) };
}

/** A payload from Rust (or anything else) as a `HostState`; `null` when it is not an object. */
export function parseHostState(raw: unknown): HostState | null {
	const o = fields(raw);
	if (!o) return null;
	const status = STATUSES.find((s) => s === o.status);
	const skew = SKEWS.find((s) => s === o.skew);
	return {
		status: (status as HostStatus | undefined) ?? "current",
		version: typeof o.version === "string" ? o.version : null,
		proto: typeof o.proto === "number" ? o.proto : null,
		caps: strs(o.caps),
		appVersion: str(o.appVersion),
		skew: (skew as HostSkew | undefined) ?? "none",
		ptys: (Array.isArray(o.ptys) ? o.ptys : []).map(parsePty).filter((p) => p !== null),
		reason: str(o.reason),
		reasonCode: str(o.reasonCode),
		reasonArgs: fields(o.reasonArgs) ?? {},
		deferred: o.deferred === true,
		idleArmed: o.idleArmed === true,
		missing: strs(o.missing),
	};
}

export type ChipTone = "warn" | "info" | "busy";

export interface Chip {
	tone: ChipTone;
	label: string;
	detail: string;
}

/** Why the host is not current, in the app's language; `""` when it is current. Falls back to the shell's English. */
export function hostReason(s: Pick<HostState, "reason" | "reasonCode" | "reasonArgs">): string {
	return s.reasonCode ? hostText({ code: s.reasonCode, args: s.reasonArgs, message: s.reason }) : s.reason;
}

/**
 * The sidebar chip: amber when the user has to act (features are off until the restart), calm
 * and neutral when only the version differs, a quiet "updating" while a handoff or restart
 * runs. `null` for a current host.
 */
export function chipFor(s: HostState): Chip | null {
	if (s.status === "upgrading" || s.status === "restarting") {
		return {
			tone: "busy",
			label: s.status === "upgrading" ? m.workbench_host_chip_updating() : m.workbench_host_chip_restarting(),
			detail: s.status === "upgrading" ? m.workbench_host_chip_terminals_stay_attached() : "",
		};
	}
	if (s.status === "outdated") {
		return {
			tone: "warn",
			label: s.idleArmed ? m.workbench_host_chip_restarts_when_idle() : m.workbench_host_chip_needs_restart(),
			detail: hostReason(s),
		};
	}
	if (s.skew === "needsRestart") {
		return {
			tone: "info",
			label: s.version
				? m.workbench_host_chip_version({ version: s.version })
				: m.workbench_host_chip_older(),
			detail: hostReason(s),
		};
	}
	return null;
}

export interface PromptCopy {
	title: string;
	/** Why the host should restart. */
	body: string;
	/** `<title|command> · <~/cwd>`, one per running terminal. */
	items: string[];
	consequence: string;
}

export function promptCopy(s: HostState): PromptCopy {
	return {
		title: m.workbench_host_prompt_title(),
		body: hostReason(s) || m.workbench_host_prompt_body(),
		items: s.ptys.map(
			(p) => `${p.title.trim() || p.argv.join(" ").trim() || "shell"} · ${tilde(p.cwd) || "~"}`,
		),
		consequence: m.workbench_host_prompt_consequence({ count: s.ptys.length }),
	};
}

/**
 * Whether a `host://prompt` may open the dialog. Rust emits the event once per daemon pid, so
 * this only drops what no longer applies by the time it arrives: the user postponed it, armed
 * "when idle", or the terminals are gone. The chip and the settings row stay visible regardless.
 */
export function shouldPrompt(s: HostState): boolean {
	return s.status === "outdated" && !s.deferred && !s.idleArmed && s.ptys.length > 0;
}
