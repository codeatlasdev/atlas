// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import {
	detailsText,
	fileNames,
	fingerprintOf,
	headline,
	isBenign,
	issueUrl,
	markdownReport,
	messageOf,
	report,
	scrub,
} from "./report.ts";

describe("isBenign", () => {
	test("ignores AbortError and the ResizeObserver loop warning", () => {
		expect(isBenign(new DOMException("x", "AbortError"))).toBe(true);
		expect(isBenign("ResizeObserver loop completed with undelivered notifications.")).toBe(true);
		expect(isBenign(new Error("The operation was aborted."))).toBe(true);
	});
	test("keeps real errors", () => {
		expect(isBenign(new TypeError("x is undefined"))).toBe(false);
		expect(isBenign("Aborting is not the same as AbortError in the middle")).toBe(false);
	});
});

describe("messageOf", () => {
	test("handles strings, errors, objects and cycles", () => {
		expect(messageOf("boom")).toBe("boom");
		expect(messageOf(new Error("bad"))).toBe("bad");
		expect(messageOf({ message: "obj" })).toBe("obj");
		const a: Record<string, unknown> = {};
		a.self = a;
		expect(messageOf(a)).toBe("[object Object]");
		expect(messageOf("  ")).toBe("Unknown error");
	});
});

describe("fileNames", () => {
	test("reduces paths and URLs to file:line:col, dropping query strings", () => {
		expect(fileNames("at f (/Users/ana/proj/src/lib/Editor.svelte?t=123:45:6)")).toBe(
			"at f (Editor.svelte:45:6)",
		);
		expect(fileNames("render@http://localhost:1420/src/lib/Editor.svelte:12:3")).toBe(
			"render@Editor.svelte:12:3",
		);
		expect(fileNames("f@[native code]")).toBe("f@[native code]");
	});
});

describe("scrub", () => {
	test("removes home directories and secret-looking query values", () => {
		expect(scrub("ENOENT /Users/ana/x/y")).toBe("ENOENT ~/x/y");
		expect(scrub("GET /a?token=abc123&b=1")).toBe("GET /a?token=…&b=1");
	});
});

describe("report + issueUrl", () => {
	const e = new Error("Cannot read properties of undefined (reading 'x') in /Users/ana/p");
	e.stack = `Error: boom\n    at render (/Users/ana/p/src/Editor.svelte:10:2)\n    at run (/Users/ana/p/src/a.ts:1:1)`;
	const r = { ...report(e, { where: "Editor", kind: "editor", file: "main.rs" }), version: "0.5.14" };

	test("keeps file names only and no home path", () => {
		expect(r.stack).toBe("    at render (Editor.svelte:10:2)\n    at run (a.ts:1:1)");
		expect(r.message).not.toContain("/Users");
	});
	test("issue URL targets the repo and carries version, message and trimmed stack", () => {
		const u = new URL(issueUrl(r));
		expect(u.origin + u.pathname).toBe("https://github.com/codeatlasdev/atlas/issues/new");
		// The repo's bug_report.yml is a form: its field ids are the parameters (`body` is ignored by forms).
		expect(u.searchParams.get("template")).toBe("bug_report.yml");
		expect(u.searchParams.get("version")).toBe("atlas 0.5.14");
		const logs = u.searchParams.get("logs") ?? "";
		expect(logs).toContain("Atlas: 0.5.14");
		expect(logs).toContain("Editor.svelte:10:2");
		expect(logs).not.toContain("/Users");
		expect(u.searchParams.get("title")?.startsWith("[desktop] Cannot read")).toBe(true);
	});
	test("details text is plain and complete", () => {
		const t = detailsText(r);
		expect(t).toContain("Atlas 0.5.14");
		expect(t).toContain("Where: Editor (editor)");
		expect(t).toContain("File: main.rs");
	});
	// The stack is clipped by frames and by line length: both shapes of "too long" stay under the URL budget. (The line is
	// words, not one 50k run of `x`: `redactText` takes time quadratic in a long run of token characters, 780 ms for
	// 50k, which timed this test out on a loaded machine; a real frame has separators.)
	test("long stacks stay under the URL budget", () => {
		const frames = { ...r, stack: "    at f (a.ts:1:1)\n".repeat(5000) };
		const oneLongLine = { ...r, stack: "    at f (a.ts:1:1) ".repeat(2500) };
		expect(issueUrl(frames).length).toBeLessThan(9000);
		expect(issueUrl(oneLongLine).length).toBeLessThan(9000);
	});
});

describe("fingerprintOf", () => {
	test("the same error at another line, port or version is one fingerprint (it folds a toast burst, and names the bug in an issue)", () => {
		const a = {
			message: "Port 24101 is in use (atlas 1.5.0-alpha.27)",
			stack: "    at start (start.ts:12:3)",
		};
		const b = { message: "Port 3000 is in use (atlas 1.6.0)", stack: "    at start (start.ts:99:1)" };
		expect(fingerprintOf(a)).toBe(fingerprintOf(b));
		expect(fingerprintOf(a)).not.toBe(fingerprintOf({ ...a, message: "Something else" }));
		expect(fingerprintOf(a)).not.toBe(fingerprintOf({ ...a, stack: "    at stop (stop.ts:1:1)" }));
	});
	test("a report that carries one (a crash file's) keeps it", () => {
		expect(fingerprintOf({ message: "m", stack: "", fingerprint: "0123456789abcdef" })).toBe(
			"0123456789abcdef",
		);
	});
});

describe("the fingerprint in the report", () => {
	const r = { message: "Port 24101 is in use", stack: "    at start (start.ts:12:3)", where: "Editor" };
	const fp = fingerprintOf(r);
	test("every rendering says it, the smallest one included, so a search by it finds the issue", () => {
		expect(markdownReport(r)).toContain(`Fingerprint: \`${fp}\``);
		expect(markdownReport(r, 1)).toContain(`Fingerprint: \`${fp}\``);
		expect(new URL(issueUrl(r)).searchParams.get("logs")).toContain(`Fingerprint: \`${fp}\``);
	});
	test("the one a report carries is the one it says", () => {
		expect(markdownReport({ ...r, fingerprint: "0123456789abcdef" })).toContain(
			"Fingerprint: `0123456789abcdef`",
		);
	});
});

test("a plain object carrying a stack (SvelteKit's handleError result) keeps its frames", () => {
	const r = report({ message: "m", stack: "m\n    at f (/Users/ana/p/a.ts:1:2)" }, { where: "x" });
	expect(r.stack).toBe("    at f (a.ts:1:2)");
});

// --- AI-first artifacts -------------------------------------------------------------------------

import {
	aiDiagnosePrompt,
	aiFixPrompt,
	aiIssuePrompt,
	causeHint,
	markdownReport,
	withContext,
} from "./report.ts";

const CTX = {
	os: "macOS 26.0",
	arch: "arm64",
	locale: "pt-BR",
	route: "/projects/atlas",
	hostVersion: "0.6.2",
	hostCaps: ["pty_attach_raw"],
	breadcrumbs: [
		{ at: 1000, text: "opened Settings > Languages" },
		{ at: 1200, text: "lsp_catalog ok" },
	],
};
const base = {
	...report(
		Object.assign(new TypeError("undefined is not an object (evaluating 't.entry.languageLabels[R(i)]')"), {
			stack:
				"TypeError: x\n    at render (/Users/ana/p/src/lib/settings/ServerRow.svelte:30:5)\n    at run (/Users/ana/p/a.ts:1:1)",
		}),
		{ where: "route /", kind: undefined, file: undefined },
	),
	version: "1.3.0",
};

describe("withContext", () => {
	test("keeps what is safe and caps breadcrumbs; scrubs home paths in breadcrumb text", () => {
		const r = withContext(base, {
			...CTX,
			breadcrumbs: Array.from({ length: 80 }, (_, i) => ({ at: i, text: `open /Users/ana/secret/${i}` })),
		});
		expect(r.context?.os).toBe("macOS 26.0");
		expect(r.context?.breadcrumbs.length).toBeLessThanOrEqual(20);
		expect(JSON.stringify(r.context)).not.toContain("/Users/ana");
		// Most recent last, and the newest are the ones kept.
		expect(r.context?.breadcrumbs.at(-1)?.at).toBe(79);
	});
});

describe("causeHint", () => {
	test("a TypeError on a missing field of a backend payload points at a Rust/TS contract mismatch", () => {
		const h = causeHint(base);
		expect(h?.title).toMatch(/backend|host|data/i);
		expect(h?.detail).toContain("ServerRow.svelte");
	});
	test("an old terminal host with an lsp error says to restart the host", () => {
		const r = withContext({ ...base, message: "lsp_open failed: unknown command" }, CTX);
		expect(causeHint(r)?.title).toMatch(/terminal host/i);
	});
	test("unknown errors have no guess rather than a wrong one", () => {
		expect(causeHint({ ...base, message: "boom", stack: "" })).toBeNull();
	});
});

describe("markdownReport", () => {
	const r = withContext(base, CTX);
	test("is a complete, paste-ready report", () => {
		const md = markdownReport(r);
		expect(md).toContain("# Atlas desktop error");
		expect(md).toContain("languageLabels");
		expect(md).toContain("Atlas: 1.3.0");
		expect(md).toContain("macOS 26.0");
		expect(md).toContain("opened Settings > Languages");
		expect(md).toContain("```");
		expect(md).not.toContain("/Users/ana");
	});
});

describe("AI prompts", () => {
	const r = withContext(base, CTX);
	test("fix prompt: names the repo, the file, the error, and demands a test first", () => {
		const p = aiFixPrompt(r);
		expect(p).toContain("codeatlasdev/atlas");
		expect(p).toContain("ServerRow.svelte:30:5");
		expect(p).toContain("languageLabels");
		expect(p).toMatch(/failing test/i);
		expect(p).toMatch(/serialized|real .*JSON|contract/i);
		expect(p).not.toContain("/Users/ana");
	});
	test("issue prompt: asks the agent to search duplicates then file with gh, body inlined", () => {
		const p = aiIssuePrompt(r);
		expect(p).toContain("gh issue list");
		expect(p).toContain("gh issue create");
		expect(p).toContain("--repo codeatlasdev/atlas");
		expect(p).toContain("languageLabels");
		expect(p).toContain("--label bug");
	});
	test("diagnose prompt: for a person, plain language, no repo edits", () => {
		const p = aiDiagnosePrompt(r);
		expect(p).toMatch(/plain language|simple terms/i);
		expect(p).toMatch(/do not (edit|change|modify)/i);
		expect(p).toContain("languageLabels");
	});
	test("prompts stay bounded however long the stack is", () => {
		const long = withContext({ ...r, stack: "at f (a.ts:1:1)\n".repeat(5000) }, CTX);
		for (const p of [aiFixPrompt(long), aiIssuePrompt(long), aiDiagnosePrompt(long)])
			expect(p.length).toBeLessThan(12_000);
	});
});

test("the issue URL carries the richer report (context + hint) yet stays under budget", () => {
	const r = withContext(base, CTX);
	const u = new URL(issueUrl(r));
	const body = u.searchParams.get("logs") ?? "";
	expect(body).toContain("macOS 26.0");
	expect(body).toContain("opened Settings > Languages");
	expect(issueUrl(r).length).toBeLessThan(9000);
});

// Regression: 1.5.0-alpha.26 titled every non-host failure "An action didn't finish", even an uncaught timer error
// the person never started. The title names what happened; host failures keep their own.
describe("headline", () => {
	const titles = (origin, where = "window", kind) => headline({ origin, where, kind }).title;
	test("uncaught error, rejection, command and host each read differently", () => {
		const all = [
			titles("error"),
			titles("rejection"),
			titles("command", "restart terminal host"),
			titles(undefined, "terminal host", "hostd"),
		];
		expect(new Set(all).size).toBe(4);
		expect(all).toEqual([
			m.errors_headline_error_title(),
			m.errors_headline_rejection_title(),
			m.errors_headline_restart_host_title(),
			m.errors_headline_host_title(),
		]);
		expect(all.join()).not.toContain("An action didn't finish");
	});
	test("a command without a name still says a command failed", () => {
		expect(titles("command", "  ")).toBe("A command failed");
	});
	test("the hint says what happened, not only that Atlas runs on", () => {
		expect(headline({ origin: "rejection", where: "window" }).hint).toBe(m.errors_headline_rejection_hint());
		expect(headline({ origin: "error", where: "window" }).hint).toBe(m.errors_headline_error_hint());
		expect(headline({ origin: "command", where: "window" }).hint).toBe(m.errors_headline_command_hint());
	});
});
