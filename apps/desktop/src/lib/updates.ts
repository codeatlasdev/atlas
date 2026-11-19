/**
 * Everything about updates that can be decided without the DOM or Tauri: which releases the
 * "What's new" sheet shows, skip/notify-once rules, the post-update toast, the install steps
 * derived from the CLI's output, and the persisted state. `stores/updates.svelte.ts` wires it up.
 */

import { formatNumber } from "#lib/i18n/fmt.ts";
import { appLocale } from "#lib/i18n/locale.svelte.ts";
import { m } from "#lib/i18n/m.ts";

export interface ReleaseNotes {
	version: string;
	url: string;
	published_at: string;
	notes: string;
}

/** Persisted under `updates` in settings.json. */
export interface UpdatePrefs {
	/** Version the user chose to skip: no pill, no notification until a newer one ships. */
	skipped: string | null;
	/** Last version a native notification was sent for (once per version). */
	notified: string | null;
	/** Version the app ran last time: a different one means it was just updated. */
	lastSeen: string | null;
	/** Check on launch and every 6 hours. */
	auto: boolean;
}

export const EMPTY_PREFS: UpdatePrefs = { skipped: null, notified: null, lastSeen: null, auto: true };

export const CHECK_EVERY_MS = 6 * 3600 * 1000;

const str = (v: unknown): string | null => (typeof v === "string" && v ? v : null);

/** Parse whatever settings.json holds into valid prefs. */
export function parsePrefs(raw: unknown): UpdatePrefs {
	if (!raw || typeof raw !== "object") return { ...EMPTY_PREFS };
	const r = raw as Record<string, unknown>;
	return {
		skipped: str(r.skipped),
		notified: str(r.notified),
		lastSeen: str(r.lastSeen),
		auto: r.auto !== false,
	};
}

/**
 * SemVer 2.0 precedence (`1.10.0 > 1.9.9`, `alpha.10 > alpha.9`, a pre-release sorts before its
 * release, build metadata ignored). Mirrors `atlas_core::channel::precedence`.
 */
export function compareVersions(a: string, b: string): number {
	const parse = (v: string) => {
		const [core, pre] = v.replace(/^v/, "").split("+")[0].split(/-(.*)/s);
		return {
			nums: core.split(".").map((n) => Number.parseInt(n, 10) || 0),
			pre: pre ? pre.split(".") : null,
		};
	};
	const x = parse(a);
	const y = parse(b);
	for (let i = 0; i < Math.max(x.nums.length, y.nums.length); i++) {
		const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
		if (d) return d < 0 ? -1 : 1;
	}
	if (x.pre === null || y.pre === null) return x.pre === y.pre ? 0 : x.pre === null ? 1 : -1;
	for (let i = 0; i < Math.max(x.pre.length, y.pre.length); i++) {
		const p = x.pre[i];
		const q = y.pre[i];
		// A shorter set of identifiers loses when all the shared ones are equal.
		if (p === undefined || q === undefined) return p === undefined ? -1 : 1;
		const pn = /^\d+$/.test(p);
		const qn = /^\d+$/.test(q);
		if (pn && qn) {
			const d = Number(p) - Number(q);
			if (d) return d < 0 ? -1 : 1;
		} else if (pn !== qn) {
			return pn ? -1 : 1; // numeric identifiers sort before alphanumeric ones
		} else if (p !== q) {
			return p < q ? -1 : 1;
		}
	}
	return 0;
}

/**
 * Releases the sheet shows, newest first: every one in `(installed, latest]`. When the app
 * already runs `latest` (after an update) it is just that release. Duplicates collapse.
 */
export function selectNotes(releases: ReleaseNotes[], installed: string, latest: string): ReleaseNotes[] {
	const seen = new Set<string>();
	const inRange = releases.filter((r) => {
		if (seen.has(r.version)) return false;
		seen.add(r.version);
		return installed === latest
			? r.version === latest
			: compareVersions(r.version, installed) > 0 && compareVersions(r.version, latest) <= 0;
	});
	return inRange.sort((a, b) => compareVersions(b.version, a.version));
}

/** The user skipped exactly this version (a newer one is not covered by the skip). */
export const isSkipped = (prefs: UpdatePrefs, version: string | null | undefined): boolean =>
	!!version && prefs.skipped === version;

/** Show the sidebar pill / banner: a release is available, not skipped, and no dev link. */
export const showPill = (
	available: boolean,
	linked: boolean,
	prefs: UpdatePrefs,
	version: string | null | undefined,
): boolean => available && !linked && !isSkipped(prefs, version);

/** Send a native notification: new version, not skipped, not already notified, notifications on. */
export function shouldNotify(
	prefs: UpdatePrefs,
	version: string | null | undefined,
	opts: { notificationsOn: boolean; linked: boolean },
): boolean {
	return (
		!!version &&
		opts.notificationsOn &&
		!opts.linked &&
		!isSkipped(prefs, version) &&
		prefs.notified !== version
	);
}

/**
 * Show "Updated to X" once: the app runs a version different from the one it ran last time,
 * and it is newer (a downgrade or a first launch with nothing stored is silent).
 */
export function justUpdated(prefs: UpdatePrefs, running: string): boolean {
	return !!prefs.lastSeen && compareVersions(running, prefs.lastSeen) > 0;
}

/** Re-check on window focus when auto-check is on and the last check is at least `CHECK_EVERY_MS` old. */
export function shouldRecheck(prefs: UpdatePrefs, checkedAt: number | null, now: number): boolean {
	return prefs.auto && (checkedAt === null || now - checkedAt >= CHECK_EVERY_MS);
}

export type StepId = "download" | "verify" | "install-cli" | "update-app" | "terminal-host" | "relaunch";
type StepState = "pending" | "active" | "done" | "failed";
export interface Step {
	id: StepId;
	state: StepState;
}

const STEP_IDS: StepId[] = ["download", "verify", "install-cli", "update-app", "terminal-host", "relaunch"];
const ORDER = STEP_IDS;

/** What a step is called, in the display language. Read it where it is drawn (a template), so it follows a language switch. */
export function stepLabel(id: StepId): string {
	switch (id) {
		case "download":
			return m.settings_updates_step_download();
		case "verify":
			return m.settings_updates_step_verify();
		case "install-cli":
			return m.settings_updates_step_install_cli();
		case "update-app":
			return m.settings_updates_step_update_app();
		case "terminal-host":
			return m.settings_host_name();
		case "relaunch":
			return m.settings_updates_step_relaunch();
	}
}

/** `updates://progress` payloads (parsed from the CLI's `@@atlas` lines in Rust). */
export type ProgressEvent =
	| { kind: "phase"; id: string }
	| { kind: "bytes"; done: number; total: number }
	| { kind: "rollback"; state: string }
	| { kind: "host"; state: string; live: number }
	| ({ kind: "managed" } & Managed);

/**
 * Another tool owns this Atlas. For a package, `manager`/`command` say how to update it and
 * `installable` that the app may run it (`atlas update --yes`, `pkexec` asks for the password)
 * after the user confirms. Rust: `Progress::Managed`.
 */
export interface Managed {
	how: string;
	manager?: string;
	installable?: boolean;
	command?: string;
}

/** What the install has reported so far. */
export interface Progress {
	/** Index into the steps of the phase under way (-1 before the first report). */
	at: number;
	/** Bytes of the download in the current phase (total 0 = unknown). */
	done: number;
	total: number;
	/** The CLI had to be put back because the app could not be updated. */
	rollback: "restored" | "failed" | null;
	/** How `atlas update` left the terminal host (`needs-restart` = running terminals, user decides). */
	host: { state: string; live: number } | null;
	/** Another tool owns this Atlas (`homebrew`, `package`): the CLI exited 0 without installing. */
	managed: Managed | null;
}

export const EMPTY_PROGRESS: Progress = {
	at: -1,
	done: 0,
	total: 0,
	rollback: null,
	host: null,
	managed: null,
};

/** Fold one event into the progress; phases only move forward, bytes belong to the current phase. */
export function reduceProgress(p: Progress, e: ProgressEvent): Progress {
	switch (e.kind) {
		case "phase": {
			const i = ORDER.indexOf(e.id as StepId);
			// A new phase starts a new download counter (the app's tarball after the CLI's).
			return i > p.at ? { ...p, at: i, done: 0, total: 0 } : p;
		}
		case "bytes":
			return { ...p, done: e.done, total: e.total };
		case "rollback":
			return { ...p, rollback: e.state === "restored" ? "restored" : "failed" };
		case "host":
			return { ...p, host: { state: e.state, live: e.live } };
		case "managed":
			return {
				...p,
				managed: { how: e.how, manager: e.manager, installable: e.installable, command: e.command },
			};
	}
}

/**
 * Phases from the lines an older CLI prints (it has no `@@atlas` protocol and announces its
 * work with `◆ Downloading…`, `◆ Installing…`, `Updating Atlas Desktop…`).
 * The current CLI reports phases structurally (`ProgressEvent`), so this fallback only ever sees English.
 */
export function phaseFromLog(line: string): ProgressEvent | null {
	// i18n-ignore: parsed from the updater's output (an older CLI's English lines), never shown
	if (/Downloading Atlas-|Updating Atlas Desktop/i.test(line)) return { kind: "phase", id: "update-app" };
	if (/Installing\b/i.test(line)) return { kind: "phase", id: "install-cli" };
	if (/Downloading\b/i.test(line)) return { kind: "phase", id: "download" };
	return null;
}

/**
 * How the last install ended. `managed`: the CLI exited 0 but installed nothing because another
 * tool owns this Atlas (Homebrew, a package manager with no usable command), so it is neither a
 * success nor a failure and no step ran.
 */
export type InstallOutcome = "running" | "done" | "failed" | "managed";

/**
 * The step list for the sheet and Settings. `failed` marks the active step; `done` completes
 * every step except Relaunch, which is only done once the app restarts (`relaunched`).
 * `managed` leaves every step pending: nothing was installed.
 */
export function stepsFrom(p: Progress, outcome: InstallOutcome, relaunchNeeded = false): Step[] {
	return STEP_IDS.map((id, i): Step => {
		let state: StepState;
		if (outcome === "managed") return { id, state: "pending" };
		if (id === "relaunch") state = outcome === "done" && relaunchNeeded ? "active" : "pending";
		else if (outcome === "done") state = "done";
		else if (i < p.at) state = "done";
		else if (i === p.at) state = outcome === "failed" ? "failed" : "active";
		else state = "pending";
		// Before anything is reported, "Download" is what is happening.
		if (p.at === -1 && i === 0 && outcome !== "done") state = outcome === "failed" ? "failed" : "active";
		return { id, state };
	});
}

/** A run that exited 0: `managed` when the CLI said another tool owns this install, else `done`. */
export function finishOutcome(p: Progress): "done" | "managed" {
	return p.managed ? "managed" : "done";
}

/** What the sheet shows for a managed install: who manages it, the command to run, whether the app can run it. */
export function managedView(managed: Managed | null): {
	title: string;
	command: string | null;
	installable: boolean;
} {
	if (managed?.how === "homebrew") {
		// Homebrew is a product name, so it is the same in every language.
		return {
			title: m.settings_updates_managed_by({ manager: "Homebrew" }),
			command: "brew upgrade atlas",
			installable: false,
		};
	}
	return {
		// `manager` is the package manager's own name (apt, pacman...), shown as the CLI reports it.
		title: managed?.manager
			? m.settings_updates_managed_by({ manager: managed.manager })
			: m.settings_updates_managed_by_package(),
		command: managed?.command ?? null,
		installable: !!managed?.command && !!managed.installable,
	};
}

/** The toast a finished run shows outside the sheet; a managed install is never "up to date". */
export function finishToast(
	kind: "update" | "reinstall" | "unlink",
	outcome: "done" | "managed",
	managed: Managed | null,
): { level: "success" | "info"; text: string } {
	if (outcome === "managed") {
		const { command, title } = managedView(managed);
		return {
			level: "info",
			text: command
				? m.settings_updates_toast_managed_command({ title, command })
				: m.settings_updates_toast_managed_package(),
		};
	}
	return {
		level: "success",
		text: kind === "unlink" ? m.settings_updates_toast_unlinked() : m.settings_updates_headline_current(),
	};
}

/** `42%` of a known download; null while the size is unknown. */
export function percent(p: Progress): number | null {
	return p.total > 0 ? Math.min(100, Math.round((p.done / p.total) * 100)) : null;
}

/** `12.4 MB` / `860 KB`; empty for an unknown size. */
export function formatBytes(n: number): string {
	if (!Number.isFinite(n) || n <= 0) return "";
	const MB = 1024 * 1024;
	if (n >= MB) {
		const digits = n >= 100 * MB ? 0 : 1;
		return `${formatNumber(n / MB, { minimumFractionDigits: digits, maximumFractionDigits: digits })} MB`;
	}
	return `${formatNumber(Math.max(1, Math.round(n / 1024)))} KB`;
}

/**
 * A release date for display. CHANGELOG dates are calendar dates (`2026-10-12`): they are
 * formatted as that local calendar day, never through `new Date("2026-10-12")`, which is UTC
 * midnight and shows the previous day west of Greenwich. Full timestamps are parsed normally.
 * Unparseable input gives "".
 */
export function dateOf(iso: string, locale: string = appLocale()): string {
	const opts: Intl.DateTimeFormatOptions = { year: "numeric", month: "short", day: "numeric" };
	const m = /^\s*(\d{4})-(\d{2})-(\d{2})\s*$/.exec(iso);
	if (m) {
		const [y, mo, d] = [Number(m[1]), Number(m[2]), Number(m[3])];
		const date = new Date(y, mo - 1, d);
		// `new Date(2026, 1, 31)` rolls over to March: reject impossible dates.
		if (date.getFullYear() !== y || date.getMonth() !== mo - 1 || date.getDate() !== d) return "";
		return date.toLocaleDateString(locale, opts);
	}
	const date = new Date(iso);
	return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString(locale, opts);
}

/** Drop a leading `## [x] — date` heading from a release body so the sheet's own header is not doubled. */
export function stripVersionHeading(notes: string): string {
	return notes.replace(/^\s*#{1,3}\s*\[?v?\d+\.\d+\.\d+[^\n]*\n+/, "").trim();
}

export type GroupKind = "added" | "changed" | "fixed" | "security" | "removed" | "deprecated" | "other";

export interface ChangeItem {
	/** The `**Title.**` lead of the bullet ("" when the bullet has none). */
	title: string;
	/** The rest of the bullet, markdown. */
	body: string;
}

export interface ChangeGroup {
	kind: GroupKind;
	/** The `###` heading as written. */
	heading: string;
	items: ChangeItem[];
}

export interface ParsedNotes {
	/** First paragraph before the first `###` (alpha boilerplate removed), "" when none. */
	summary: string;
	/** The "Atlas is in alpha" paragraph, shown as a footer note. */
	alpha: string;
	groups: ChangeGroup[];
}

const KINDS: Record<string, GroupKind> = {
	added: "added",
	new: "added",
	changed: "changed",
	improved: "changed",
	fixed: "fixed",
	security: "security",
	removed: "removed",
	deprecated: "deprecated",
};

/** Keep a Changelog heading → kind; unknown headings are `other`. */
export const kindOf = (heading: string): GroupKind => KINDS[heading.trim().toLowerCase()] ?? "other";

/** A group heading of the notes: the catalog's word for a known kind, the changelog's own heading for `other`. */
export function kindLabel(kind: GroupKind, heading: string): string {
	switch (kind) {
		case "added":
			return m.settings_updates_whats_new_kind_added();
		case "changed":
			return m.settings_updates_whats_new_kind_changed();
		case "fixed":
			return m.settings_updates_whats_new_kind_fixed();
		case "security":
			return m.settings_updates_whats_new_kind_security();
		case "removed":
			return m.settings_updates_whats_new_kind_removed();
		case "deprecated":
			return m.settings_updates_whats_new_kind_deprecated();
		case "other":
			return heading;
	}
}

const ALPHA = /\balpha\b/i;

/** `**Title.** body` / `**Title:** body` / `**Title** — body` → title + body; otherwise no title. */
export function splitItem(text: string): ChangeItem {
	const m = /^\*\*(.+?)\*\*\s*([\s\S]*)$/.exec(text.trim());
	if (!m) return { title: "", body: text.trim() };
	const title = m[1].replace(/[.:]\s*$/, "").trim();
	const body = m[2].replace(/^[—–-]\s+/, "").trim();
	// A bold phrase that is really the start of the sentence ("**Never** do X") has no body to split off.
	return body ? { title, body } : { title: "", body: text.trim() };
}

/**
 * A release's markdown (our CHANGELOG convention) as `{ summary, alpha, groups }`:
 * paragraphs before the first `###` are the summary (the alpha boilerplate is split out),
 * each `###` heading is a group, each top-level bullet an item (continuation lines and nested
 * bullets stay in the item's body).
 */
export function parseNotes(markdown: string): ParsedNotes {
	const lines = stripVersionHeading(markdown).split("\n");
	const intro: string[] = [];
	const groups: ChangeGroup[] = [];
	let group: ChangeGroup | null = null;
	let item: string[] | null = null;
	const flush = () => {
		if (group && item) group.items.push(splitItem(item.join("\n")));
		item = null;
	};
	for (const line of lines) {
		const h = /^#{3}\s+(.+?)\s*$/.exec(line);
		if (h) {
			flush();
			group = { kind: kindOf(h[1]), heading: h[1], items: [] };
			groups.push(group);
		} else if (!group) {
			intro.push(line);
		} else if (/^[-*]\s+/.test(line)) {
			flush();
			item = [line.replace(/^[-*]\s+/, "")];
		} else if (item && line.trim()) {
			// Continuation (indented or lazy); an indented bullet stays a nested list line.
			item.push(/^\s+[-*]\s+/.test(line) ? `\n- ${line.replace(/^\s+[-*]\s+/, "")}` : line.trim());
		} else if (!item && line.trim()) {
			// Prose under a heading without bullets becomes an item of its own.
			item = [line.trim()];
		}
	}
	flush();
	const paras = intro
		.join("\n")
		.split(/\n\s*\n/)
		.map((p) => p.replace(/\s*\n\s*/g, " ").trim())
		.filter(Boolean);
	return {
		summary: paras.filter((p) => !ALPHA.test(p)).join(" "),
		alpha: paras.find((p) => ALPHA.test(p)) ?? "",
		groups: groups.filter((g) => g.items.length > 0),
	};
}

/** Bodies longer than this fold behind "More" in the sheet. */
export const FOLD_AT = 190;

/** Whether an item's body is long enough to fold. */
export const isLong = (i: ChangeItem): boolean => i.body.length > FOLD_AT;

/**
 * The install is `behind` (an update is on offer), `new` (just installed it), or `current`
 * (it is what runs); drives the badges of a release's hero.
 */
export type ReleaseTag = "latest" | "installed" | "new";

/** Badges for `version` given the running version, the latest one and whether it was just installed. */
export function badgesFor(
	version: string,
	running: string | undefined,
	latest: string | undefined,
	justInstalled: boolean,
): ReleaseTag[] {
	const tags: ReleaseTag[] = [];
	if (latest && version === latest) tags.push("latest");
	if (justInstalled && running === version) tags.push("new");
	else if (running === version) tags.push("installed");
	return tags;
}

/** The text of a release badge, in the display language. */
export function badgeLabel(tag: ReleaseTag): string {
	switch (tag) {
		case "latest":
			return m.settings_updates_whats_new_badge_latest();
		case "new":
			return m.settings_updates_whats_new_badge_new();
		case "installed":
			return m.settings_updates_whats_new_badge_installed();
	}
}

/** DOM id of a version's block (`1.2.1` → `v-1-2-1`), for the sticky index. */
export const versionAnchor = (version: string): string => `v-${version.replace(/[^0-9a-z]+/gi, "-")}`;

/**
 * Inline markdown as the text a reader would see: links keep their label, code spans their content, bold/italic lose
 * their markers. For surfaces that cannot render HTML (the "Updated to X" card, a notification).
 */
export function plainText(markdown: string): string {
	return markdown
		.replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
		.replace(/`([^`]*)`/g, "$1")
		.replace(/(\*\*|__)(.+?)\1/g, "$2")
		.replace(/(^|[\s(])[*_]([^*_\s][^*_]*?)[*_](?=$|[\s).,;:!?])/g, "$1$2")
		.replace(/\s+/g, " ")
		.trim();
}

/**
 * One line about a release for the "Updated to X" card: the changelog's own summary, else the
 * title (or first words) of the first change, as plain text. "" when the release lists nothing.
 */
export function blurbOf(n: ParsedNotes): string {
	if (n.summary) return plainText(n.summary);
	const first = n.groups[0]?.items[0];
	if (!first) return "";
	const text = plainText(first.title || first.body);
	return text.length > 90 ? `${text.slice(0, 89).trimEnd()}…` : text;
}

// ── Update channels ─────────────────────────────────────────────────────────────────────────
// Mirrors `atlas_core::channel` (serialized by `updates::update_channels` / `update_channel_get`).

export type Channel = "stable" | "beta" | "alpha";
export const CHANNELS: readonly Channel[] = ["stable", "beta", "alpha"];
/**
 * What a machine follows before its user picks a channel: the variant marked `#[default]` on
 * `atlas_core::channel::Channel` (a test reads that file). Used only while the real state
 * (`update_channel_get`) has not arrived; Alpha until the first Stable that meets the GA criteria.
 */
export const DEFAULT_CHANNEL: Channel = "alpha";
export type ChannelAction = "none" | "update" | "downgrade";
export type Risk = "low" | "medium" | "high";

/** `atlas_core::channel::ChannelView`, snake_case as serialized. */
export interface ChannelView {
	channel: Channel;
	label: string;
	promise: string;
	risk: Risk;
	risk_label: string;
	cadence: string;
	audience: string;
	switch_to: string;
	alpha_era_note: string | null;
	selected: boolean;
	tag: string | null;
	offered: string | null;
	action: ChannelAction;
}

/** `updates::ChannelState`. */
export interface ChannelState {
	channel: Channel;
	stage: string;
	stage_note: string;
	env_override: boolean;
}

const isChannel = (v: unknown): v is Channel => CHANNELS.includes(v as Channel);

function fail(what: string): never {
	throw new Error(`unexpected ${what} from the backend`);
}

const needStr = (o: Record<string, unknown>, k: string, what: string): string =>
	typeof o[k] === "string" ? (o[k] as string) : fail(`${what}.${k}`);
const optStr = (o: Record<string, unknown>, k: string, what: string): string | null =>
	o[k] === null ? null : needStr(o, k, what);

/** Validate the `update_channels` payload (throws when the shape drifts from the Rust side). */
export function parseChannelViews(raw: unknown): ChannelView[] {
	if (!Array.isArray(raw)) fail("channel list");
	return raw.map((r): ChannelView => {
		if (typeof r !== "object" || r === null) fail("channel");
		const o = r as Record<string, unknown>;
		if (!isChannel(o.channel)) fail("channel.channel");
		if (o.risk !== "low" && o.risk !== "medium" && o.risk !== "high") fail("channel.risk");
		if (o.action !== "none" && o.action !== "update" && o.action !== "downgrade") fail("channel.action");
		if (typeof o.selected !== "boolean") fail("channel.selected");
		return {
			channel: o.channel,
			label: needStr(o, "label", "channel"),
			promise: needStr(o, "promise", "channel"),
			risk: o.risk,
			risk_label: needStr(o, "risk_label", "channel"),
			cadence: needStr(o, "cadence", "channel"),
			audience: needStr(o, "audience", "channel"),
			switch_to: needStr(o, "switch_to", "channel"),
			alpha_era_note: optStr(o, "alpha_era_note", "channel"),
			selected: o.selected,
			tag: optStr(o, "tag", "channel"),
			offered: optStr(o, "offered", "channel"),
			action: o.action,
		};
	});
}

/** Validate the `update_channel_get` / `update_channel_set` payload. */
export function parseChannelState(raw: unknown): ChannelState {
	if (typeof raw !== "object" || raw === null) fail("channel state");
	const o = raw as Record<string, unknown>;
	if (!isChannel(o.channel)) fail("state.channel");
	if (typeof o.env_override !== "boolean") fail("state.env_override");
	return {
		channel: o.channel,
		stage: needStr(o, "stage", "state"),
		stage_note: needStr(o, "stage_note", "state"),
		env_override: o.env_override,
	};
}

/**
 * The channel a build of `version` was cut for (`rc` counts as beta); `null` for a label Atlas
 * does not ship. Mirrors `Channel::of_version`.
 */
export function channelOfVersion(version: string): Channel | null {
	const m = /^v?\d+\.\d+\.\d+(?:-([0-9A-Za-z-]+)(?:\.[0-9A-Za-z.-]+)?)?(?:\+.*)?$/.exec(version.trim());
	if (!m) return null;
	if (!m[1]) return "stable";
	if (m[1] === "alpha") return "alpha";
	return m[1] === "beta" || m[1] === "rc" ? "beta" : null;
}

/** The text fields of a channel that the backend sends as English copy (`atlas_core::channel`, `data/channels.json`). */
export type ChannelTextField =
	"label" | "promise" | "risk_label" | "cadence" | "switch_to" | "alpha_era_note";

/**
 * The catalog's message for a producer text field of channel `id`, `null` when there is none. The copy of a channel
 * is written once in `channels.json` (English, also printed by the CLI and read by the website); the desktop shows
 * its own translation of it, picked by the stable channel id and never by matching the English. A contract test
 * (`updates.test.ts`) holds every message equal to the producer's English, so the two cannot drift.
 */
export function channelText(id: string, field: ChannelTextField): string | null {
	switch (`${id}.${field}`) {
		case "stable.label":
			return m.settings_updates_channel_stable_label();
		case "stable.promise":
			return m.settings_updates_channel_stable_promise();
		case "stable.risk_label":
			return m.settings_updates_channel_stable_risk();
		case "stable.cadence":
			return m.settings_updates_channel_stable_cadence();
		case "stable.switch_to":
			return m.settings_updates_channel_stable_switch_to();
		case "stable.alpha_era_note":
			return m.settings_updates_channel_stable_alpha_era_note();
		case "beta.label":
			return m.settings_updates_channel_beta_label();
		case "beta.promise":
			return m.settings_updates_channel_beta_promise();
		case "beta.risk_label":
			return m.settings_updates_channel_beta_risk();
		case "beta.cadence":
			return m.settings_updates_channel_beta_cadence();
		case "beta.switch_to":
			return m.settings_updates_channel_beta_switch_to();
		case "alpha.label":
			return m.settings_updates_channel_alpha_label();
		case "alpha.promise":
			return m.settings_updates_channel_alpha_promise();
		case "alpha.risk_label":
			return m.settings_updates_channel_alpha_risk();
		case "alpha.cadence":
			return m.settings_updates_channel_alpha_cadence();
		case "alpha.switch_to":
			return m.settings_updates_channel_alpha_switch_to();
		default:
			return null;
	}
}

/** `field` of a channel in the display language; the producer's own text for an id or field the catalog does not know. */
export function channelCopy(v: ChannelView, field: Exclude<ChannelTextField, "alpha_era_note">): string;
export function channelCopy(v: ChannelView, field: "alpha_era_note"): string | null;
export function channelCopy(v: ChannelView, field: ChannelTextField): string | null {
	const raw = v[field];
	return raw === null ? null : (channelText(v.channel, field) ?? raw);
}

/** A channel's name (Stable, Beta, Alpha) in the display language. */
export function channelName(id: Channel): string {
	return channelText(id, "label") ?? id;
}

/** The catalog's message for the product stage note (`stage`: `alpha` today), `null` for a stage it does not know. */
export function stageText(stage: string): string | null {
	return stage === "alpha" ? m.settings_updates_channel_stage_alpha_note() : null;
}

/** The note above the channel selector: the catalog's message for the stage, else the producer's text. */
export function stageNote(s: Pick<ChannelState, "stage" | "stage_note">): string {
	return stageText(s.stage) ?? s.stage_note;
}

/** A prerelease section of the changelog: `alpha`, `beta` or a release candidate. */
export type Prerelease = "alpha" | "beta" | "rc";

/** `alpha` / `beta` / `rc` for a prerelease section; `null` for a release or an unknown label. */
export function prereleaseBadge(version: string): Prerelease | null {
	if (channelOfVersion(version) === null) return null;
	const label = /-(alpha|beta|rc)\b/.exec(version)?.[1];
	return label === "alpha" || label === "beta" || label === "rc" ? label : null;
}

/** The text of a prerelease badge (Alpha, Beta, RC), in the display language. */
export function prereleaseLabel(p: Prerelease): string {
	return p === "rc" ? m.settings_updates_whats_new_pre_rc() : channelName(p);
}

/** The live offer of a card ("Offers 1.4.0-beta.2" / "Nothing newer than what you run" / …). */
export function offerLine(v: Pick<ChannelView, "action" | "offered">): string {
	if (!v.offered) return m.settings_updates_channel_offer_unknown();
	if (v.action === "update") return m.settings_updates_channel_offers({ offered: v.offered });
	if (v.action === "downgrade") return m.settings_updates_channel_offers_older({ offered: v.offered });
	return m.settings_updates_channel_offer_none();
}

export type Tone = "success" | "warn" | "error";
const RISK_TONE: Record<Risk, Tone> = { low: "success", medium: "warn", high: "error" };

export interface Card {
	view: ChannelView;
	/** The channel this app follows. */
	following: boolean;
	/** The running build was cut for this channel. */
	installed: boolean;
	tone: Tone;
	offer: string;
}

/** Everything a card renders that is not copy: selection, installed marker, risk colour, offer. */
export function cardState(views: ChannelView[], running: string | undefined, selected: Channel): Card[] {
	const own = running ? channelOfVersion(running) : null;
	return views.map((view) => ({
		view,
		following: view.channel === selected,
		installed: own === view.channel,
		tone: RISK_TONE[view.risk],
		offer: offerLine(view),
	}));
}

export interface Confirm {
	/** What switching means (the channel's `switch_to`). */
	text: string;
	/** The explicit, separate downgrade action: only when the channel offers something older. */
	downgrade: { label: string; version: string; warning: string } | null;
}

/** Copy of the inline confirmation panel. Switching alone never downgrades. */
export function confirmCopy(v: ChannelView): Confirm {
	const down =
		v.action === "downgrade" && v.offered
			? {
					label: m.settings_updates_install_downgrade({ version: v.offered }),
					version: v.offered,
					warning: m.settings_updates_downgrade_warning(),
				}
			: null;
	return { text: channelCopy(v, "switch_to"), downgrade: down };
}

export type Lane = "followed" | "pending" | "feeds" | "idle";

/**
 * State of each lane in the release-train diagram (alpha → beta → stable). Builds promote down the train,
 * so a channel *feeds* the focused one when it sits upstream of it. The focus is the reader's unconfirmed
 * pick (`pending`) when there is one, else the channel the app follows.
 */
export function laneStates(followed: Channel, pending: Channel | null): Record<Channel, Lane> {
	const train: Channel[] = ["alpha", "beta", "stable"];
	const focus = pending ?? followed;
	const out = {} as Record<Channel, Lane>;
	for (const c of train) {
		out[c] =
			c === focus
				? pending
					? "pending"
					: "followed"
				: train.indexOf(c) < train.indexOf(focus)
					? "feeds"
					: "idle";
	}
	return out;
}

/** What the Settings ▸ Updates card says, one at a time. */
export type UpdateStatus =
	"restart" | "dev" | "installing" | "checking" | "available" | "current" | "unknown";

/**
 * The single state of the Updates card. A verdict ("up to date", "X is available") is about the last finished check,
 * so while a check runs there is no verdict: the card says it is checking (it once showed "Atlas is up to date" and
 * "Checking…" together). A failed check has no verdict either, whatever an earlier one found. Facts that do not depend
 * on the network (a restart is pending, a dev build, an install under way) come first and are never hidden by a check.
 */
export function updateStatus(s: {
	relaunchNeeded: boolean;
	linked: boolean;
	installing: boolean;
	checking: boolean;
	failed: boolean;
	available: boolean;
	checked: boolean;
}): UpdateStatus {
	if (s.relaunchNeeded) return "restart";
	if (s.linked) return "dev";
	if (s.installing) return "installing";
	if (s.checking) return "checking";
	if (s.failed) return "unknown";
	if (s.available) return "available";
	return s.checked ? "current" : "unknown";
}

/** The card's headline for `status`. */
export function updateHeadline(status: UpdateStatus, version: string | null | undefined): string {
	switch (status) {
		case "restart":
			return m.settings_updates_headline_restart();
		case "dev":
			return m.settings_updates_headline_dev();
		case "checking":
			return m.settings_updates_headline_checking();
		case "available":
			return m.tray_recommend_update_title({ version: version ?? "" });
		case "current":
			return m.settings_updates_headline_current();
		case "installing":
			return version
				? m.settings_updates_headline_installing_to({ version })
				: m.settings_updates_headline_installing();
		case "unknown":
			return m.settings_updates_headline_unknown();
	}
}
