/**
 * Turns anything that was thrown into an `ErrorReport`, and an `ErrorReport` into the two things a
 * person does with it: copy the details, or file a GitHub issue. Pure functions; no DOM, no stores.
 *
 * Privacy: reports leave the machine only when the person clicks "Report issue", and even then
 * only the app version, the message and a stack trimmed to file names (no directories, no home
 * path, no env). Query strings and long hex/base64-looking tokens are dropped from messages.
 */

import { machineLines } from "./machine.ts";

export interface ErrorReport {
	/** First line of the error, what the person reads. */
	message: string;
	/** Trimmed to the frames that matter; absolute paths are reduced to `file:line:col` by `fileNames`. */
	stack: string;
	/** What was rendering: `Editor`, `Terminal`, `window`, `route /tray`… */
	where: string;
	/** Tab kind when the error happened inside a pane. */
	kind?: string;
	/** File the pane was showing (editor tabs), already reduced to its file name. */
	file?: string;
	version?: string;
	/** What the person's machine and the last few things the app did looked like (never file contents). */
	context?: ErrorContext;
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
/** GitHub rejects URLs past ~8 KB; stay well below it. */
const MAX_BODY = 4000;

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
		.replace(/([?&](?:token|key|secret|password|auth)[^=&\s]*=)[^&\s]*/gi, "$1…");
}

const NAMED_SECRET =
	/\b([\w-]*(?:token|secret|passw(?:or)?d|api[_-]?key|access[_-]?key|private[_-]?key|authorization|cookie|credential)[\w-]*)(["']?\s*[=:]\s*)(?:Bearer |Basic )?("[^"]*"|'[^']*'|[^\s,&"']+)/gi;
const TOKEN_SHAPE =
	/\b(?:gh[pousr]_|github_pat_|sk-|xox[bp]-|AKIA|glpat-|npm_|pk_live_|sk_live_)[A-Za-z0-9_-]{8,}|\b(?=[A-Za-z0-9+/_=-]*\d)(?=[A-Za-z0-9+/_=-]*[A-Za-z])[A-Za-z0-9+/_=-]{32,}/g;

/**
 * The same rules as `diag::redact` in Rust, applied again on the way out so a report cannot leak even
 * when a log field arrives unredacted: named secrets, token shapes, URL credentials, other users' homes.
 */
export function redactText(text: string): string {
	return scrub(text)
		.replace(/\/(?:Users|home)\/[^/\s"']+/g, "~")
		.replace(/(\w+:\/\/)[^\s/@]+@/g, "$1[redacted]@")
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

/** The text behind "Copy details". */
export function detailsText(r: ErrorReport): string {
	const head = [
		`Atlas ${r.version ?? "(version unknown)"}`,
		`Where: ${r.where}${r.kind ? ` (${r.kind})` : ""}`,
		r.file ? `File: ${r.file}` : "",
	].filter(Boolean);
	const d = r.context?.diagnostics;
	const diag = d
		? [
				`Terminal host: ${d.versions.hostd ?? "not running"}`,
				`CLI: ${d.versions.cli ?? "unknown"}`,
				`OS: ${d.os || "unknown"}`,
				`Running bundle: ${d.install.runningBundle ?? "(not a bundle)"}`,
				`Installed app: ${d.install.installedApp ?? "none found"}`,
				// Rust redacted these (`diag::redact`); `scrub` only drops homes and query secrets again: `redactText`'s
				// token shape reads `WEBKIT_DISABLE_DMABUF_RENDERER=1` and an AppImage mount path as secrets.
				...machineLines(d).map(scrub),
				...findings(d).map((f) => `Finding: ${f}`),
				...(d.lastCrash ? [`Last crash: ${redactText(d.lastCrash)}`] : []),
				...(d.stderrLog ? ["", "stderr.log (tail):", redactText(d.stderrLog)] : []),
				...(d.hostdLog ? ["", "hostd.log (tail):", redactText(d.hostdLog)] : []),
			]
		: [];
	return [...head, "", r.message, r.stack ? `\n${r.stack}` : "", ...(diag.length ? ["", ...diag] : [])]
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

/**
 * A best-effort, honest guess at the cause, for the screen and the prompts. Only patterns that are
 * known to happen in this app; anything else returns `null` rather than a confident wrong answer.
 */
export function causeHint(r: ErrorReport): CauseHint | null {
	const frame = topFrame(r.stack);
	const at = frame ? ` Look at ${frame.replace(/^at\s+/, "")}.` : "";
	const host = r.context?.hostCaps;
	if (/\blsp_\w+|language server|unknown (command|cmd)/i.test(r.message) && host && !host.includes("lsp"))
		return {
			title: "The terminal host is older than this app",
			detail:
				"The background terminal host (atlas hostd) predates language-server support. Restart it from the status bar; terminals running in it will end.",
		};
	if (/undefined is not an object|cannot read propert|is undefined|is not a function/i.test(r.message))
		return {
			title: "The app received data in a shape it did not expect",
			detail: `A field the screen reads is missing, typically a name mismatch between the Rust backend (serde) and the frontend.${at}`,
		};
	if (/failed to fetch|networkerror|load failed|csp|content security/i.test(r.message))
		return {
			title: "A request was blocked or the backend did not answer",
			detail:
				"Check the Atlas desktop logs (`atlas desktop debug logs`) for a CSP violation or an IPC failure.",
		};
	return null;
}

const ctxLines = (r: ErrorReport): string[] => {
	const c = r.context;
	return [
		`- Atlas: ${r.version ?? "unknown"}`,
		`- Where: ${r.where}${r.kind ? ` (${r.kind})` : ""}`,
		r.file ? `- File: ${r.file}` : "",
		c?.os ? `- OS: ${c.os}${c.arch ? ` (${c.arch})` : ""}` : "",
		c?.locale ? `- Locale: ${c.locale}` : "",
		c?.route ? `- Route: ${c.route}` : "",
		c?.hostVersion || c?.diagnostics?.versions.hostd
			? `- Terminal host: ${c.hostVersion ?? c.diagnostics?.versions.hostd}${c.hostCaps ? ` [${c.hostCaps.join(", ")}]` : ""}`
			: "",
		c?.diagnostics?.versions.cli ? `- CLI: ${c.diagnostics.versions.cli}` : "",
		!c?.os && c?.diagnostics?.os ? `- OS: ${c.diagnostics.os}` : "",
	].filter(Boolean);
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

const diagLines = (r: ErrorReport): string[] => {
	const d = r.context?.diagnostics;
	if (!d) return [];
	const v = d.versions;
	return [
		"",
		"## Versions",
		`- App: ${v.app || "unknown"}`,
		`- CLI: ${v.cli ?? "unknown"}`,
		`- Terminal host: ${v.hostd ?? "not running"}${v.hostdPid ? ` (pid ${v.hostdPid})` : ""}`,
		`- OS: ${d.os || "unknown"}`,
		`- Channel: ${d.install.channel || "unknown"}`,
		"",
		"## Install",
		`- Running bundle: ${d.install.runningBundle ?? "(not a bundle)"}`,
		`- Installed app: ${d.install.installedApp ?? "none found"}`,
		`- CLI: ${d.install.cli ?? "not found"}${d.install.cliLinkedTo ? ` -> ${d.install.cliLinkedTo}` : ""}`,
		`- Quarantined: ${d.install.quarantined ? "yes" : "no"}; translocated: ${d.install.translocated ? "yes" : "no"}`,
		"",
		"## Machine",
		...machineLines(d).map((l) => `- ${scrub(l)}`),
		...(findings(d).length ? ["", "## Findings", ...findings(d).map((f) => `- ${f}`)] : []),
		"",
		...(d.lastCrash ? [`- Last crash: ${redactText(d.lastCrash)}`] : []),
		...(d.incidents.length
			? [
					"",
					"## Host incidents",
					...d.incidents
						.slice(-8)
						.map((i) => `- ${new Date(i.at * 1000).toISOString().slice(11, 19)} ${i.kind}: ${i.detail}`),
				]
			: []),
		// stderr first: it is what explains a blank window, and the limit cuts from the end.
		...(d.stderrLog ? ["", "## stderr.log (tail)", ...fence(redactText(d.stderrLog))] : []),
		...(d.hostdLog ? ["", "## hostd.log (tail)", ...fence(redactText(d.hostdLog))] : []),
		...(d.appLog ? ["", "## app.log (tail)", ...fence(redactText(d.appLog))] : []),
	];
};

const crumbLines = (r: ErrorReport): string[] =>
	(r.context?.breadcrumbs ?? []).map((b) => `- ${new Date(b.at).toISOString().slice(11, 19)} ${b.text}`);

const fence = (t: string) => ["```", t, "```"];

/** A complete, paste-ready report: what an issue or an AI conversation starts from. */
export function markdownReport(r: ErrorReport, limit = MAX_BODY): string {
	const hint = causeHint(r);
	const crumbs = crumbLines(r);
	return [
		"# Atlas desktop error",
		"",
		"## Error",
		...fence(r.message.slice(0, 600)),
		"",
		"## Environment",
		...ctxLines(r),
		...(hint ? ["", "## Likely cause", `**${hint.title}.** ${hint.detail}`] : []),
		...(crumbs.length ? ["", "## Last things the app did", ...crumbs] : []),
		...diagLines(r),
		"",
		"## Stack (file names only)",
		...fence(r.stack || "(none)"),
	]
		.join("\n")
		.slice(0, limit);
}

/** Longest `logs` field in the URL: GitHub rejects URLs past ~8 KB. */
const MAX_LOGS = 2500;

/**
 * Prefilled "new issue" URL for `.github/ISSUE_TEMPLATE/bug_report.yml`: its field ids (`what`, `surface`,
 * `version`, `os`, `logs`...) are query parameters, so the form opens filled. Everything is redacted first.
 * Opens in the system browser through `api.openUrl`.
 */
export function issueUrl(r: ErrorReport): string {
	const title = redactText(`[desktop] ${r.message.split("\n")[0]}`).slice(0, MAX_TITLE);
	const d = r.context?.diagnostics;
	const version = r.version ?? d?.versions.app;
	const q = new URLSearchParams({
		template: "bug_report.yml",
		title,
		labels: "type: bug",
		what: redactText(`${r.message}\n\n(What were you doing when this appeared?)`).slice(0, 800),
		surface: "Atlas Desktop",
		...(version ? { version: `atlas ${version}` } : {}),
		...(d?.os || r.context?.os ? { os: d?.os ?? r.context?.os ?? "" } : {}),
		logs: redactText(
			[
				markdownReport({ ...r, context: r.context && { ...r.context, diagnostics: undefined } }, 1200),
				d?.hostdLog ?? "",
			]
				.filter(Boolean)
				.join("\n\n"),
		).slice(-MAX_LOGS),
	});
	return `${REPO_URL}/issues/new?${q}`;
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
	]
		.join("\n")
		.slice(0, PROMPT_CAP);
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
	]
		.join("\n")
		.slice(0, PROMPT_CAP);
}

/** For a person using any chat AI: explain it, say what to do now, no code changes. */
export function aiDiagnosePrompt(r: ErrorReport): string {
	return [
		"I use Atlas (a desktop app for running dev projects) and it showed this error screen. Explain what happened in plain language, what I can do right now to get back to work, and whether it is a bug I should report. Do not edit, change or run anything on my machine; only explain.",
		"",
		markdownReport(r, PROMPT_CAP - 600),
	]
		.join("\n")
		.slice(0, PROMPT_CAP);
}

/** Same error fired in a burst (a render loop, a timer) should surface once. */
export function fingerprint(r: Pick<ErrorReport, "message" | "stack">): string {
	return `${r.message}\n${r.stack.split("\n")[0] ?? ""}`;
}
