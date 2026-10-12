/**
 * Everything about updates that can be decided without the DOM or Tauri: which releases the
 * "What's new" sheet shows, skip/notify-once rules, the post-update toast, the install steps
 * derived from the CLI's output, and the persisted state. `stores/updates.svelte.ts` wires it up.
 */

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

/** Numeric semver compare (`1.10.0 > 1.9.9`; a pre-release sorts before its release). */
export function compareVersions(a: string, b: string): number {
	const parse = (v: string) => {
		const [core, pre] = v.replace(/^v/, "").split("+")[0].split(/-(.*)/s);
		return { nums: core.split(".").map((n) => Number.parseInt(n, 10) || 0), pre: pre ?? null };
	};
	const x = parse(a);
	const y = parse(b);
	for (let i = 0; i < Math.max(x.nums.length, y.nums.length); i++) {
		const d = (x.nums[i] ?? 0) - (y.nums[i] ?? 0);
		if (d) return d < 0 ? -1 : 1;
	}
	if (x.pre === y.pre) return 0;
	if (x.pre === null) return 1;
	if (y.pre === null) return -1;
	return x.pre < y.pre ? -1 : 1;
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

export type StepId = "download" | "install" | "refresh";
export type StepState = "pending" | "active" | "done" | "failed";
export interface Step {
	id: StepId;
	label: string;
	state: StepState;
}

const STEPS: { id: StepId; label: string }[] = [
	{ id: "download", label: "Download and verify" },
	{ id: "install", label: "Install" },
	{ id: "refresh", label: "Update the app" },
];

/**
 * Install progress from the lines `atlas update` prints (it announces each phase with `◆`).
 * `failed` marks the active step as failed; `done` completes every step.
 */
export function stepsFromLog(log: string[], outcome: "running" | "done" | "failed"): Step[] {
	let at = -1;
	for (const line of log) {
		if (/Downloading/i.test(line)) at = Math.max(at, 0);
		else if (/Installing\b/i.test(line)) at = Math.max(at, 1);
		else if (/Updated to|Refreshing skills|Updating Atlas Desktop|Atlas Desktop v/i.test(line))
			at = Math.max(at, 2);
	}
	// The CLI checks the download's checksum inside the download step, so it prints no line of its own.
	return STEPS.map((s, i) => {
		let state: StepState;
		if (outcome === "done") state = "done";
		else if (i < at) state = "done";
		else if (i === at) state = outcome === "failed" ? "failed" : "active";
		else state = "pending";
		// Before the first line arrives, "Download" is what is happening.
		if (at === -1 && i === 0 && outcome === "running") state = "active";
		if (at === -1 && i === 0 && outcome === "failed") state = "failed";
		return { ...s, state };
	});
}

/** First `## [x.y.z]` heading's date out of a CHANGELOG-style heading (`## [1.0.1] — 2026-10-11`). */
export function dateOf(iso: string): string {
	const d = new Date(iso);
	if (Number.isNaN(d.getTime())) return "";
	return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** Drop a leading `## [x] — date` heading from a release body so the sheet's own header is not doubled. */
export function stripVersionHeading(notes: string): string {
	return notes.replace(/^\s*#{1,3}\s*\[?v?\d+\.\d+\.\d+[^\n]*\n+/, "").trim();
}
