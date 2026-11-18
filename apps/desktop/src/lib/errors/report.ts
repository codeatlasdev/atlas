/**
 * Turns anything that was thrown into an `ErrorReport`, and an `ErrorReport` into the two things a
 * person does with it: copy the details, or file a GitHub issue. Pure functions; no DOM, no stores.
 *
 * Privacy: reports leave the machine only when the person clicks "Report issue", and even then
 * only the app version, the message and a stack trimmed to file names (no directories, no home
 * path, no env). Query strings and long hex/base64-looking tokens are dropped from messages.
 */

import { m } from "#lib/i18n/m.ts";
import { machineLines } from "./machine.ts";

/** `{ locale: "en" }` for text that leaves the machine (an issue, an AI prompt), nothing for the screen. */
type Lang = { locale: "en" } | undefined;

/** The `where` of the failed restart of the terminal host (`errors.restartHost`): an identifier, never shown as is. */
export const RESTART_HOST = "restart terminal host";

export type ErrorOrigin = "error" | "rejection" | "command";

export interface ErrorReport {
	/** First line of the error, what the person reads. */
	message: string;
	/** Trimmed to the frames that matter; absolute paths are reduced to `file:line:col` by `fileNames`. */
	stack: string;
	/** What was rendering: `Editor`, `Terminal`, `window`, `route /tray`… */
	where: string;
	/** Tab kind when the error happened inside a pane. */
	kind?: string;
	/** How it surfaced: a script `error`, an unhandled promise `rejection`, or a `command` the person started. */
	origin?: ErrorOrigin;
	/** File the pane was showing (editor tabs), already reduced to its file name. */
	file?: string;
	version?: string;
	/** What the person's machine and the last few things the app did looked like (never file contents). */
	context?: ErrorContext;
	/** Title and hint of a report whose wording lives with its producer (the crash card), so the startup chunk does not carry it. */
	headline?: { title: string; hint: string };
}

/** One thing the app did shortly before the error (a command, a host call, a navigation). */
export interface Breadcrumb {
	at: number;
	text: string;
}

/** What `host_diagnostics` returns (`crates/atlas-desktop/src/diag.rs`, camelCase; see `diagnostics.ts`). */
export interface Diagnostics {
	versions: {
		app: string;
		cli: string | null;
		hostd: string | null;
		hostdPid: number | null;
		hostdCaps: string[];
	};
	os: string;
	install: {
		runningBundle: string | null;
		installedApp: string | null;
		cli: string | null;
		cliLinkedTo: string | null;
		quarantined: boolean;
		translocated: boolean;
		channel: string;
	};
	/** Distro, session, GPU and the graphics stack (`atlas_core::probe::Platform`, #275). */
	platform: {
		distro: string | null;
		kernel: string | null;
		session: string | null;
		desktop: string | null;
		compositor: string | null;
		gpus: string[];
		nvidiaDriver: string | null;
		mesa: string | null;
		gtk: string | null;
		webview: string | null;
		graphicsEnv: string[];
	};
	/** How Atlas got onto the machine (`atlas_core::probe::Packaging`). */
	packaging: {
		appimage: string | null;
		exe: string | null;
		exeOwner: string | null;
		localBin: string[];
	};
	/** Core dumps of the last day and Atlas's own crash files, with when they happened (`probe::Crashes`). */
	crashes: {
		/** `ok`, `unavailable`, or empty where there is no systemd-coredump to ask. */
		coredumpStatus: string;
		coredumps: { at: number; pid: number; signal: string; exe: string }[];
		/** Newest first; `ageSecs` is measured when the report was built. */
		atlas: { at: number; ageSecs: number; summary: string }[];
	};
	hostdLog: string;
	appLog: string;
	/** What GTK, WebKit and the web processes printed (`desktop/stderr.log`, #273). */
	stderrLog: string;
	lastCrash: string | null;
	incidents: { at: number; kind: string; detail: string }[];
}

export interface ErrorContext {
	/** Versions, OS and log tails of the app, CLI and terminal host; set for host failures and on demand. */
	diagnostics?: Diagnostics;
	os?: string;
	arch?: string;
	locale?: string;
	/** Route of the window (`/`, `/tray`). */
	route?: string;
	/** Terminal host (`atlas hostd`) version and capabilities, when it answered. */
	hostVersion?: string;
	hostCaps?: string[];
	/** Oldest first, newest last. */
	breadcrumbs: Breadcrumb[];
}

export const REPO_URL = "https://github.com/codeatlasdev/atlas";
const MAX_STACK_LINES = 12;
const MAX_TITLE = 100;
/** GitHub rejects a "new issue" URL past ~8 KB (8192 bytes); the whole URL, encoded, stays under this. */
export const MAX_URL = 7500;

/** `AbortError` and WebKit's resize loop warning are normal operation, not bugs. */
const BENIGN = [
	/ResizeObserver loop/i,
	/^AbortError\b/,
	/The operation was aborted/i,
	/The user aborted a request/i,
];

export function isBenign(e: unknown, message = messageOf(e)): boolean {
	if (e instanceof DOMException && e.name === "AbortError") return true;
	if (e instanceof Error && e.name === "AbortError") return true;
	return BENIGN.some((r) => r.test(message));
}

export function messageOf(e: unknown): string {
	let m: string;
	if (typeof e === "string") m = e;
	else if (e instanceof Error) m = e.message || e.name;
	else if (e && typeof e === "object" && "message" in e) m = String(e.message);
	else {
		try {
			m = JSON.stringify(e) ?? String(e);
		} catch {
			m = String(e);
		}
	}
	return m.trim() || "Unknown error";
}

/** `/Users/x/proj/src/lib/Editor.svelte?t=123:45:6` → `Editor.svelte:45:6`; `(native)` stays. */
export function fileNames(stack: string): string {
	return stack
		.split("\n")
		.map((line) =>
			line.replace(
				/(?:[a-z][a-z0-9+.-]*:\/\/[^\s/)]*|\/)[^\s)]*?([^/\s)?:]+)(?:\?[^\s):]*)?(:\d+(?::\d+)?)/gi,
				"$1$2",
			),
		)
		.join("\n");
}

/** `Error.stack`, or the `stack` string SvelteKit's `handleError` result carries across the boundary. */
function stackOf(e: unknown): string {
	if (e instanceof Error) return e.stack ?? "";
	if (e && typeof e === "object" && "stack" in e && typeof e.stack === "string") return e.stack;
	return "";
}

/** Stack with the message line removed, file names only, capped. */
export function trimStack(e: unknown): string {
	const raw = stackOf(e);
	const lines = fileNames(raw)
		.split("\n")
		.map((l) => l.trimEnd())
		.filter(Boolean);
	// V8 prefixes the stack with "Name: message"; WebKit does not. Drop it when present.
	const frames = lines[0] && !/^\s*at\s|@/.test(lines[0]) ? lines.slice(1) : lines;
	return frames.slice(0, MAX_STACK_LINES).join("\n");
}

/** Strips a home directory and query strings; the message is shown and sent, so keep it tame. */
export function scrub(text: string): string {
	return text
		.replace(/\/Users\/[^/\s]+/g, "~")
		.replace(/\/home\/[^/\s]+/g, "~")
		.replace(/([?&](?:token|key|secret|password|auth)[^=&?\s]*=)[^&\s]*/gi, "$1…");
}

// Each rule below starts only where a run of its characters starts (`(?<!…)`), never at every position inside one:
// a rule that looks ahead or reads to the end of the run from each position costs the square of a base64 blob or a
// minified line (100k characters took 1.5 s, and 17 s when the run had separators). The prefixed tokens keep `\b`
// because they stop at the prefix check, and so match after a `/` or `.` inside a longer run. The name of a named
// secret is taken whole and without backtracking (`(?=(…))\1`): it ends where the run ends, so giving characters back
// only retries the separator at each of them.
const NAMED_SECRET =
	/(?<![\w-])(?=[\w-]*?(?:token|secret|passw(?:or)?d|api[_-]?key|access[_-]?key|private[_-]?key|authorization|cookie|credential))(?=([\w-]+))\1(["']?\s*[=:]\s*)(?:Bearer |Basic )?("[^"]*"|'[^']*'|[^\s,&"']+)/gi;
const TOKEN_SHAPE =
	/\b(?:gh[pousr]_|github_pat_|sk-|xox[bp]-|AKIA|glpat-|npm_|pk_live_|sk_live_)[A-Za-z0-9_-]{8,}|(?<![A-Za-z0-9+/_=-])(?=[A-Za-z0-9+/_=-]*\d)(?=[A-Za-z0-9+/_=-]*[A-Za-z])[A-Za-z0-9+/_=-]{32,}/g;

/**
 * The same rules as `diag::redact` in Rust, applied again on the way out so a report cannot leak even
 * when a log field arrives unredacted: named secrets, token shapes, URL credentials, other users' homes.
 */
export function redactText(text: string): string {
	return scrub(text)
		.replace(/\/(?:Users|home)\/[^/\s"']+/g, "~")
		.replace(/(?<!\w)(\w+:\/\/)[^\s/@]+@/g, "$1[redacted]@")
		.replace(NAMED_SECRET, (_m, name, sep) => `${name}${sep}[redacted]`)
		.replace(TOKEN_SHAPE, "[redacted]");
}

export function report(e: unknown, ctx: Pick<ErrorReport, "where"> & Partial<ErrorReport>): ErrorReport {
	return {
		...ctx,
		message: scrub(messageOf(e)),
		stack: scrub(trimStack(e)),
	};
}

/** What a toast or the details dialog calls an error: the title names what happened, the hint what still works. */
export function headline(r: Pick<ErrorReport, "kind" | "origin" | "where" | "headline">): {
	title: string;
	hint: string;
} {
	if (r.headline) return r.headline;
	if (r.kind === "hostd")
		return { title: m.errors_headline_host_title(), hint: m.errors_headline_host_hint() };
	if (r.origin === "rejection")
		return { title: m.errors_headline_rejection_title(), hint: m.errors_headline_rejection_hint() };
	if (r.origin === "command") {
		const what = r.where.trim();
		// The one command that reports (`errors.restartHost`) has its own sentence; any other shows its name as given.
		const title =
			what === RESTART_HOST
				? m.errors_headline_restart_host_title()
				: what
					? m.errors_headline_command_title({ what: `${what[0].toUpperCase()}${what.slice(1)}` })
					: m.errors_headline_command_unnamed_title();
		return { title, hint: m.errors_headline_command_hint() };
	}
	return { title: m.errors_headline_error_title(), hint: m.errors_headline_error_hint() };
}

/** The text behind "Copy details". */
export function detailsText(r: ErrorReport): string {
	const head = [
		`Atlas ${r.version ?? "(version unknown)"}`,
		`Where: ${r.where}${r.kind ? ` (${r.kind})` : ""}`,
		r.file ? `File: ${r.file}` : "",
	].filter(Boolean);
	const d = r.context?.diagnostics;
	const tails = d
		? [
				["app.log", d.appLog],
				["stderr.log", d.stderrLog],
				["hostd.log", d.hostdLog],
			].flatMap(([name, text]) => (text ? ["", `${name} (tail):`, redactText(text)] : []))
		: [];
	const diag = d
		? [
				`Terminal host: ${d.versions.hostd ?? "not running"}`,
				`CLI: ${d.versions.cli ?? "unknown"}`,
				`OS: ${d.os || "unknown"}`,
				`Running bundle: ${d.install.runningBundle ?? "(not a bundle)"}`,
				`Installed app: ${d.install.installedApp ?? "none found"}`,
				// Machine lines: `atlas_core::redact::redact` already redacted them in `atlas_core::probe`; `scrub` only drops
				// homes and query secrets again (`redactText`'s token shape reads `WEBKIT_DISABLE_DMABUF_RENDERER=1` as a secret).
				...machineLines(d).map(scrub),
				...findings(d).map((f) => `Finding: ${redactText(f)}`),
				...(d.lastCrash ? [`Last crash: ${redactText(d.lastCrash)}`] : []),
				...tails,
			]
		: [];
	return [
		...head,
		"",
		redactText(r.message),
		r.stack ? `\n${redactText(r.stack)}` : "",
		...(diag.length ? ["", ...diag] : []),
	]
		.join("\n")
		.trimEnd();
}

const MAX_BREADCRUMBS = 20;
const MAX_CRUMB = 160;

/** Attach machine/app context; every free-text field is scrubbed and bounded. */
export function withContext(
	r: ErrorReport,
	c: Partial<ErrorContext> & { breadcrumbs?: Breadcrumb[] },
): ErrorReport {
	const crumbs = (c.breadcrumbs ?? [])
		.slice(-MAX_BREADCRUMBS)
		.map((b) => ({ at: b.at, text: scrub(b.text).slice(0, MAX_CRUMB) }));
	return { ...r, context: { ...c, breadcrumbs: crumbs } };
}

export interface CauseHint {
	title: string;
	/** One or two sentences: what it probably is and what to do about it. */
	detail: string;
}

/** The frame that most likely points at the culprit: first one inside the app (not a library). */
function topFrame(stack: string): string | undefined {
	return stack
		.split("\n")
		.map((l) => l.trim())
		.find((l) => /\.(svelte|ts|js)[:)]/.test(l) && !/node_modules/.test(l));
}

type Platform = "macos" | "linux" | "windows";

/** The reporter's OS, from the Rust label (`Linux (x86_64)`) or the page's user agent. */
function platformOf(r: ErrorReport): Platform | undefined {
	const s = `${r.context?.diagnostics?.os ?? ""} ${r.context?.os ?? ""}`;
	if (/mac/i.test(s)) return "macos";
	if (/windows/i.test(s)) return "windows";
	if (/linux/i.test(s)) return "linux";
	return undefined;
}

/**
 * Where to read the app's logs, per OS. `atlas desktop debug logs` is not an option: it is compiled out of release
 * builds. `stderr.log` is written on Linux only.
 */
function blockedDetail(platform: Platform | undefined, lang: Lang): string {
	switch (platform) {
		case "linux":
			return m.errors_cause_blocked_detail_linux({}, lang);
		case "macos":
			return m.errors_cause_blocked_detail_macos({}, lang);
		case "windows":
			return m.errors_cause_blocked_detail_windows({}, lang);
		default:
			return m.errors_cause_blocked_detail_other({}, lang);
	}
}

/**
 * A best-effort, honest guess at the cause, for the screen and the prompts. Only patterns that are
 * known to happen in this app; anything else returns `null` rather than a confident wrong answer.
 * The screen reads it in the app's language; a report that leaves the machine passes `"en"`.
 */
export function causeHint(r: ErrorReport, locale?: "en"): CauseHint | null {
	const lang: Lang = locale ? { locale } : undefined;
	const frame = topFrame(r.stack)?.replace(/^at\s+/, "");
	const host = r.context?.hostCaps;
	if (/\blsp_\w+|language server|unknown (command|cmd)/i.test(r.message) && host && !host.includes("lsp"))
		return {
			title: m.errors_cause_old_host_title({}, lang),
			detail: m.errors_cause_old_host_detail({}, lang),
		};
	if (/undefined is not an object|cannot read propert|is undefined|is not a function/i.test(r.message))
		return {
			title: m.errors_cause_shape_title({}, lang),
			detail: frame ? m.errors_cause_shape_detail_at({ frame }, lang) : m.errors_cause_shape_detail({}, lang),
		};
	if (/failed to fetch|networkerror|load failed|csp|content security/i.test(r.message))
		return {
			title: m.errors_cause_blocked_title({}, lang),
			detail: blockedDetail(platformOf(r), lang),
		};
	return null;
}

/** The whole environment of the report: versions, OS, install and the last crash. Never cut to fit a budget. */
const envLines = (r: ErrorReport): string[] => {
	const c = r.context;
	const d = c?.diagnostics;
	const v = d?.versions;
	const os = d?.os || (c?.os ? `${c.os}${c.arch ? ` (${c.arch})` : ""}` : "");
	const host = c?.hostVersion ?? v?.hostd;
	const caps = c?.hostCaps ?? (v?.hostdCaps.length ? v.hostdCaps : undefined);
	// Quarantine and translocation are macOS facts: on Linux and Windows "no" is noise.
	const macFacts = d && (platformOf(r) === "macos" || d.install.quarantined || d.install.translocated);
	return [
		`- Atlas: ${r.version ?? (v?.app || "unknown")}`,
		`- Where: ${r.where}${r.kind ? ` (${r.kind})` : ""}`,
		r.file ? `- File: ${r.file}` : "",
		os ? `- OS: ${os}` : "",
		c?.locale ? `- Locale: ${c.locale}` : "",
		c?.route ? `- Route: ${c.route}` : "",
		host || d
			? `- Terminal host: ${host ?? "not running"}${v?.hostdPid ? ` (pid ${v.hostdPid})` : ""}${caps ? ` [${caps.join(", ")}]` : ""}`
			: "",
		d ? `- CLI: ${v?.cli ?? "unknown"}` : "",
		d ? `- Channel: ${d.install.channel || "unknown"}` : "",
		d ? `- Running bundle: ${d.install.runningBundle ?? "(not a bundle)"}` : "",
		d ? `- Installed app: ${d.install.installedApp ?? "none found"}` : "",
		d
			? `- CLI path: ${d.install.cli ?? "not found"}${d.install.cliLinkedTo ? ` -> ${d.install.cliLinkedTo}` : ""}`
			: "",
		macFacts
			? `- Quarantined: ${d.install.quarantined ? "yes" : "no"}; translocated: ${d.install.translocated ? "yes" : "no"}`
			: "",
		d?.lastCrash ? `- Last crash: ${d.lastCrash}` : "",
	]
		.filter(Boolean)
		.map(redactText)
		.concat(
			// Machine facts (distro, GPU, packaging, core dumps): `atlas_core::redact::redact` already redacted every
			// path and variable in `atlas_core::probe` (`graphics_env`, `packaging_of`, `parse_coredumps`,
			// `crash_summary`). `scrub` only drops homes and query secrets again: `redactText`'s token shape reads
			// `WEBKIT_DISABLE_DMABUF_RENDERER=1` and an AppImage mount path as secrets.
			d ? machineLines(d).map((l) => `- ${scrub(l)}`) : [],
		);
};

/**
 * What differs on this machine from a healthy install, one sentence each. Only facts the payload proves; no guesses.
 * These lead the report, so a maintainer sees the likely cause before the logs.
 */
export function findings(d: Diagnostics): string[] {
	const { app, cli, hostd } = d.versions;
	const out: string[] = [];
	if (hostd && app && hostd !== app) out.push(`Terminal host ${hostd} is older than Atlas ${app}.`);
	if (cli && app && cli !== app)
		out.push(
			`CLI ${cli} differs from the app ${app} (an update did not finish, or the app is linked to a dev build).`,
		);
	if (d.install.translocated)
		out.push(
			"The app runs from a macOS translocated copy (App Translocation): move Atlas.app to /Applications and reopen it.",
		);
	if (d.install.cliLinkedTo) out.push(`The CLI is linked to a dev build (${d.install.cliLinkedTo}).`);
	if (
		d.install.runningBundle &&
		d.install.installedApp &&
		d.install.runningBundle !== d.install.installedApp &&
		!d.install.translocated
	)
		out.push(
			`The running app (${d.install.runningBundle}) is not the installed one (${d.install.installedApp}).`,
		);
	return out;
}

/** What one rendering of the report may hold. The Environment is in none of it: it is never cut. */
interface Budget {
	/** Characters of the error message. */
	msg: number;
	/** Frames of the stack, from the top. */
	stack: number;
	/** Last lines of each log. */
	app: number;
	stderr: number;
	hostd: number;
	crumbs: number;
	incidents: number;
	/** The "Likely cause" paragraph. */
	hint: boolean;
	/** Characters of a stack frame or a log line. */
	line: number;
}

/**
 * From everything (what the clipboard gets) down to the least that is still useful. A size-limited output (the
 * issue URL, an AI prompt) takes the first step that fits, so it loses the host tail and the breadcrumbs first, then
 * stack frames, and only then lines of app.log and stderr.log; never half a fence, never the Environment (#274).
 */
const STEPS: Budget[] = [
	{
		msg: Infinity,
		stack: Infinity,
		app: Infinity,
		stderr: Infinity,
		hostd: Infinity,
		crumbs: Infinity,
		incidents: Infinity,
		hint: true,
		line: Infinity,
	},
	{ msg: 600, stack: 12, app: 20, stderr: 20, hostd: 10, crumbs: 8, incidents: 8, hint: true, line: 200 },
	{ msg: 600, stack: 12, app: 20, stderr: 20, hostd: 5, crumbs: 5, incidents: 4, hint: true, line: 200 },
	{ msg: 400, stack: 8, app: 20, stderr: 20, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 200 },
	{ msg: 300, stack: 6, app: 20, stderr: 20, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 160 },
	{ msg: 300, stack: 4, app: 20, stderr: 20, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 120 },
	{ msg: 300, stack: 4, app: 15, stderr: 15, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 120 },
	{ msg: 300, stack: 3, app: 10, stderr: 10, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 100 },
	{ msg: 200, stack: 2, app: 5, stderr: 5, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 100 },
	{ msg: 200, stack: 2, app: 3, stderr: 3, hostd: 0, crumbs: 0, incidents: 0, hint: false, line: 80 },
];

/** The first rendering that passes `ok`; the smallest one when none does. */
function fit<T>(make: (b: Budget) => T, ok: (t: T) => boolean): T {
	let out!: T;
	for (const b of STEPS) {
		out = make(b);
		if (ok(out)) return out;
	}
	return out;
}

const lastN = <T>(a: T[], n: number) => a.slice(Math.max(0, a.length - n));
const clip = (s: string, max: number) => (s.length > max ? `${s.slice(0, max - 1)}…` : s);

/** A code fence that the text itself cannot close: longer than any run of backticks inside it. */
const fence = (t: string) => {
	const ticks = "`".repeat(Math.max(3, ...[...t.matchAll(/`{3,}/g)].map((m) => m[0].length + 1)));
	return [ticks, t, ticks];
};

const crumbLines = (r: ErrorReport): string[] =>
	(r.context?.breadcrumbs ?? []).map(
		(b) => `- ${new Date(b.at).toISOString().slice(11, 19)} ${redactText(b.text)}`,
	);

/** The report as Markdown within `b`. Every section is redacted on its own, before it is fenced, and cut by whole lines. */
function render(r: ErrorReport, b: Budget): string {
	const d = r.context?.diagnostics;
	const hint = b.hint ? causeHint(r, "en") : null;
	const found = d ? findings(d) : [];
	const stack = redactText(r.stack)
		.split("\n")
		.slice(0, b.stack)
		.map((l) => clip(l, b.line));
	const tails: [string, string, number][] = d
		? [
				["app.log", d.appLog, b.app],
				["stderr.log", d.stderrLog, b.stderr],
				["hostd.log", d.hostdLog, b.hostd],
			]
		: [];
	return [
		"# Atlas desktop error",
		"",
		"## Error",
		...fence(redactText(clip(r.message, b.msg))),
		"",
		"## Environment",
		...envLines(r),
		...(found.length ? ["", "## Findings", ...found.map((f) => `- ${redactText(f)}`)] : []),
		...(hint ? ["", "## Likely cause", `**${hint.title}.** ${hint.detail}`] : []),
		"",
		"## Stack (file names only)",
		...fence(r.stack && b.stack ? stack.join("\n") : "(none)"),
		...tails.flatMap(([name, text, n]) => {
			const lines = lastN(redactText(text).trimEnd().split("\n"), n).map((l) => clip(l, b.line));
			return text.trim() && lines.length ? ["", `## ${name} (tail)`, ...fence(lines.join("\n"))] : [];
		}),
		...(d?.incidents.length && b.incidents
			? [
					"",
					"## Host incidents",
					...lastN(d.incidents, b.incidents).map(
						(i) =>
							`- ${new Date(i.at * 1000).toISOString().slice(11, 19)} ${i.kind}: ${redactText(i.detail)}`,
					),
				]
			: []),
		...(b.crumbs && r.context?.breadcrumbs.length
			? ["", "## Last things the app did", ...lastN(crumbLines(r), b.crumbs)]
			: []),
	].join("\n");
}

/**
 * A complete, paste-ready report: what an issue or an AI conversation starts from. Without `limit` it is whole
 * (the clipboard); with one, the largest rendering of at most `limit` characters.
 */
export function markdownReport(r: ErrorReport, limit = Infinity): string {
	return fit(
		(b) => render(r, b),
		(md) => md.length <= limit,
	);
}

/**
 * `encodeURIComponent` that leaves the characters a query may carry as they are (`:` `/` `,` `@` `;` `$` `[` `]`)
 * and writes a space as `+`: logs are mostly those, and every `%XX` costs two bytes of an 8 KB URL.
 */
const queryValue = (s: string) =>
	encodeURIComponent(s)
		.replace(/%20/g, "+")
		.replace(/%(?:3A|2F|2C|40|3B|24|5B|5D)/g, (m) => String.fromCharCode(Number.parseInt(m.slice(1), 16)));

/** Release channels `bug_report.yml` offers; a dropdown only prefills with one of its option names. */
const FORM_CHANNELS = ["stable", "beta", "alpha"];

/**
 * Prefilled "new issue" URL for `.github/ISSUE_TEMPLATE/bug_report.yml`: its field ids (`what`, `surface`,
 * `version`, `os`, `logs`...) are query parameters, so the form opens filled. Everything is redacted first; the
 * `logs` field is the report with the largest budget whose whole URL stays within `MAX_URL`.
 * Opens in the system browser through `api.openUrl`.
 */
export function issueUrl(r: ErrorReport): string {
	const title = redactText(`[desktop] ${r.message.split("\n")[0]}`).slice(0, MAX_TITLE);
	const d = r.context?.diagnostics;
	const version = r.version ?? d?.versions.app;
	const channel = d?.install.channel ?? "";
	const fields: Record<string, string> = {
		template: "bug_report.yml",
		title,
		labels: "type: bug",
		what: `${clip(redactText(r.message), 300)}\n\n(What were you doing when this appeared?)`,
		surface: "Atlas Desktop",
		...(version ? { version: `atlas ${version}` } : {}),
		...(d?.os || r.context?.os ? { os: d?.os ?? r.context?.os ?? "" } : {}),
		...(FORM_CHANNELS.includes(channel) ? { channel } : {}),
	};
	return fit(
		(b) =>
			`${REPO_URL}/issues/new?${Object.entries({ ...fields, logs: render(r, b) })
				.map(([k, v]) => `${k}=${queryValue(v)}`)
				.join("&")}`,
		(url) => url.length <= MAX_URL,
	);
}

const PROMPT_CAP = 9000;
const REPO = "codeatlasdev/atlas";

/** For a coding agent opened in an Atlas checkout: reproduce, test first, fix. */
export function aiFixPrompt(r: ErrorReport): string {
	return [
		`You are fixing a bug in Atlas desktop (${REPO}: Rust workspace in crates/*, SvelteKit frontend in apps/desktop). Read AGENTS.md first and follow its "Tests first" rule.`,
		"",
		"Process:",
		"1. Find the root cause from the report below (start at the first app frame of the stack).",
		"2. Write a failing test that reproduces it BEFORE touching the fix, and run it to watch it fail. If the bug is a Rust(serde)/TypeScript contract mismatch, the test must exercise the real serialized JSON, not a hand-written mock.",
		"3. Think through edge cases of the same root cause and add tests for them.",
		"4. Fix the root cause (not the symptom), show the tests pass, then run `bun run check`.",
		"",
		markdownReport(r, PROMPT_CAP - 1200),
	].join("\n");
}

/** For an agent with `gh`: dedupe, then file a well-formed issue. */
export function aiIssuePrompt(r: ErrorReport): string {
	const title = `[desktop] ${r.message.split("\n")[0]}`.slice(0, MAX_TITLE).replace(/"/g, "'");
	return [
		`File a GitHub issue for this Atlas desktop error in ${REPO}. Do not change any code.`,
		"",
		`1. Search for an existing issue first: \`gh issue list --repo ${REPO} --state all --search "${title.replace(/^\[desktop\] /, "").slice(0, 60)}"\`. If one matches, comment on it with the environment below instead of opening a new one.`,
		`2. Otherwise create it: \`gh issue create --repo ${REPO} --label bug --title "${title}" --body-file -\` and use the report below as the body, adding a short "What I was doing" section from my description (ask me for it if you do not have it).`,
		"3. Reply with the issue URL.",
		"",
		markdownReport(r, PROMPT_CAP - 1200),
	].join("\n");
}

/** For a person using any chat AI: explain it, say what to do now, no code changes. */
export function aiDiagnosePrompt(r: ErrorReport): string {
	return [
		"I use Atlas (a desktop app for running dev projects) and it showed this error screen. Explain what happened in plain language, what I can do right now to get back to work, and whether it is a bug I should report. Do not edit, change or run anything on my machine; only explain.",
		"",
		markdownReport(r, PROMPT_CAP - 600),
	].join("\n");
}

/** Same error fired in a burst (a render loop, a timer) should surface once. */
export function fingerprint(r: Pick<ErrorReport, "message" | "stack">): string {
	return `${r.message}\n${r.stack.split("\n")[0] ?? ""}`;
}
