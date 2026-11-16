// @ts-nocheck bun test runtime types are not part of the app tsconfig
// #274: a desktop report reaches GitHub through a URL of at most ~8 KB. It used to cut the environment block,
// drop app.log and leave a code fence open; "Copy as Markdown" was cut at the same 4000 characters.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import fixture from "../__fixtures__/diagnostics.json";
import { hostErrorReport, parseDiagnostics } from "./diagnostics.ts";
import { causeHint, detailsText, issueUrl, markdownReport, withContext } from "./report.ts";

/** What GitHub accepts for a whole "new issue" URL. */
const GITHUB_URL_LIMIT = 8000;

/** One realistic log line: timestamp, level, a module path and a sentence, ~110 characters. */
const line = (tag, i) =>
	`2026-11-15T10:${String(i).padStart(2, "0")}:03Z ${tag} WARN atlas_desktop::host: line ${i} retrying connection to ~/.atlas/hostd.sock`;
const tail = (tag, n = 40) => Array.from({ length: n }, (_, i) => line(tag, i)).join("\n");

/** A Linux machine whose three tails are full (40 lines, as Rust sends them) and whose stack is long. */
const linux = (over = {}) =>
	parseDiagnostics({
		...fixture,
		os: "Linux (x86_64)",
		install: {
			...fixture.install,
			runningBundle: "/opt/atlas/Atlas.AppImage",
			installedApp: null,
			quarantined: false,
		},
		hostdLog: tail("hostd"),
		appLog: tail("app"),
		stderrLog: tail("stderr"),
		...over,
	});

const LONG_STACK = Array.from(
	{ length: 40 },
	(_, i) => `    at renderComponentLongName${i} (SomeDeeplyNestedComponent${i}.svelte:${i + 1}:13)`,
).join("\n");

const reportFor = (d, over = {}) => ({
	...hostErrorReport("Terminal host not reachable. Use Restart terminal host.", d),
	stack: LONG_STACK,
	...over,
});

/** CommonMark fences: an opener of N backticks closes only on a line of at least N backticks. True when none is left open. */
function fencesBalanced(md) {
	let open = 0;
	for (const l of md.split("\n")) {
		const m = /^ {0,3}(`{3,})(.*)$/.exec(l);
		if (!m) continue;
		if (open === 0) open = m[1].length;
		else if (m[1].length >= open && m[2].trim() === "") open = 0;
	}
	return open === 0;
}

const logsOf = (url) => new URL(url).searchParams.get("logs") ?? "";

describe("the issue URL at the real budget keeps what a maintainer needs", () => {
	const d = linux();
	const url = issueUrl(reportFor(d));
	const logs = logsOf(url);

	test("stays under GitHub's limit with three full tails and a long stack", () => {
		expect(url.length).toBeLessThan(GITHUB_URL_LIMIT);
	});

	test("the environment block is complete", () => {
		expect(logs).toContain("## Environment");
		for (const s of [
			"- Atlas: 1.5.0-alpha.5",
			"- OS: Linux (x86_64)",
			"- Terminal host: 1.5.0-alpha.4",
			"- CLI: 1.5.0-alpha.5",
			"- Channel: alpha",
			"- Running bundle: /opt/atlas/Atlas.AppImage",
			"- Installed app: none found",
		])
			expect(logs).toContain(s);
	});

	test("the last 20 lines of app.log and of stderr.log are there, in order", () => {
		for (const tag of ["app", "stderr"]) {
			expect(logs).toContain(`## ${tag}.log (tail)`);
			const at = (i) => logs.indexOf(`line ${i} retrying`, logs.indexOf(`${tag} WARN`));
			for (let i = 20; i < 40; i++) expect(logs).toContain(line(tag, i));
			expect(logs).toContain(line(tag, 39));
			expect(at(21)).toBeGreaterThan(at(20));
		}
	});

	test("the error and the top of the stack are there and every fence is closed", () => {
		expect(logs).toContain("Terminal host not reachable");
		expect(logs).toContain("renderComponentLongName0");
		expect(fencesBalanced(logs)).toBe(true);
	});

	test("a Linux report does not carry macOS-only install facts", () => {
		expect(logs).not.toContain("Quarantined");
	});

	test("the form opens with the channel the app knows, not an empty dropdown", () => {
		expect(new URL(url).searchParams.get("channel")).toBe("alpha");
	});
});

describe("whatever the sizes, the URL fits and each part stays whole", () => {
	const sizes = [0, 1, 5, 20, 40, 400];
	for (const logLines of sizes)
		for (const stackLines of [0, 3, 40, 400]) {
			test(`tails of ${logLines} lines, stack of ${stackLines} lines`, () => {
				const d = linux({
					hostdLog: tail("hostd", logLines),
					appLog: tail("app", logLines),
					stderrLog: tail("stderr", logLines),
				});
				const stack = LONG_STACK.split("\n").concat(LONG_STACK.split("\n"), LONG_STACK.split("\n"));
				const url = issueUrl(reportFor(d, { stack: stack.slice(0, stackLines * 1).join("\n") }));
				const logs = logsOf(url);
				expect(url.length).toBeLessThan(GITHUB_URL_LIMIT);
				expect(fencesBalanced(logs)).toBe(true);
				expect(logs).toContain("## Environment");
				expect(logs).toContain("- OS: Linux (x86_64)");
				expect(logs).toContain("- Channel: alpha");
				if (logLines >= 1) {
					expect(logs).toContain(line("app", logLines - 1));
					expect(logs).toContain(line("stderr", logLines - 1));
				}
			});
		}

	test("a log line that is itself a code fence cannot close the section early", () => {
		const d = linux({ appLog: `${tail("app", 5)}\n\`\`\`\nrm -rf ~\n\`\`\``, stderrLog: "```" });
		const r = reportFor(d, { message: "boom\n```\nmore" });
		expect(fencesBalanced(logsOf(issueUrl(r)))).toBe(true);
		expect(fencesBalanced(markdownReport(r))).toBe(true);
	});

	test("a cut never lands inside a log line", () => {
		const d = linux({
			appLog: Array.from({ length: 40 }, (_, i) => `${i} ${"x".repeat(7)}-${i}`).join("\n"),
		});
		const logs = logsOf(issueUrl(reportFor(d, { stack: "" })));
		for (const l of logs.split("\n").filter((l) => /^\d+ x{7}-/.test(l)))
			expect(l).toMatch(/^(\d+) x{7}-\1$/);
	});
});

describe("the clipboard copy is never truncated", () => {
	const d = linux();
	const r = reportFor(d, {
		stack: Array.from({ length: 200 }, (_, i) => `    at frame${i} (f.ts:${i}:1)`).join("\n"),
	});

	test("Copy as Markdown carries every line of every tail, the whole stack and balanced fences", () => {
		const md = markdownReport(r);
		expect(md.length).toBeGreaterThan(8000);
		for (const tag of ["hostd", "app", "stderr"])
			for (let i = 0; i < 40; i++) expect(md).toContain(line(tag, i));
		expect(md).toContain("frame0 (f.ts:0:1)");
		expect(md).toContain("frame199 (f.ts:199:1)");
		expect(md).toContain("- Channel: alpha");
		expect(fencesBalanced(md)).toBe(true);
	});

	test("Copy details carries app.log as well as stderr.log and hostd.log", () => {
		const t = detailsText(r);
		for (const tag of ["hostd", "app", "stderr"]) {
			expect(t).toContain(`${tag}.log (tail):`);
			for (let i = 0; i < 40; i++) expect(t).toContain(line(tag, i));
		}
	});
});

describe("redaction holds in every section, whichever way the report leaves", () => {
	const SECRET = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
	const leaky = linux({
		hostdLog: `GITHUB_TOKEN=${SECRET} in /home/bob/work\n${tail("hostd", 3)}`,
		appLog: `Authorization: Bearer abc.def.ghi at /home/bob/x\n${tail("app", 3)}`,
		stderrLog: `OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345 /home/bob\n${tail("stderr", 3)}`,
	});
	const r = withContext(reportFor(leaky, { message: `fetch failed ?token=${SECRET}` }), {
		breadcrumbs: [{ at: 1000, text: "open /home/bob/secret" }],
		diagnostics: leaky,
	});

	test("URL, Markdown and details never contain a secret or another home", () => {
		const outs = [decodeURIComponent(issueUrl(r)), markdownReport(r), detailsText(r)];
		for (const out of outs) {
			expect(out).not.toContain(SECRET);
			expect(out).not.toContain("abc.def.ghi");
			expect(out).not.toContain("sk-proj-abcdefghijklmnopqrstuvwxyz012345");
			expect(out).not.toContain("/home/bob");
		}
	});

	test("a secret that ends a log with a bare key cannot swallow the closing fence", () => {
		const d = linux({ appLog: `${tail("app", 3)}\nsecret:`, stderrLog: `${tail("stderr", 3)}\npassword=` });
		const r2 = reportFor(d);
		expect(fencesBalanced(markdownReport(r2))).toBe(true);
		expect(fencesBalanced(logsOf(issueUrl(r2)))).toBe(true);
	});
});

describe("the text names commands that exist on the reporter's platform", () => {
	const hint = (os) =>
		causeHint({
			message: "Failed to fetch",
			stack: "",
			where: "route /",
			context: { os, breadcrumbs: [] },
		});
	const text = (os) => `${hint(os).title} ${hint(os).detail}`;

	test("never the debug-only `atlas desktop debug logs`, which a release build does not have", () => {
		for (const os of ["Linux", "macOS", "Windows", undefined])
			expect(text(os)).not.toContain("desktop debug");
	});

	test("Linux and macOS get a shell tail of the real log path; Windows gets PowerShell", () => {
		expect(text("Linux")).toContain("tail -n 50 ~/.atlas/desktop/stderr.log");
		expect(text("macOS")).toContain("tail -n 50 ~/.atlas/desktop/app.log");
		expect(text("Windows")).toContain("Get-Content");
		expect(text("Windows")).not.toContain("tail -n");
		expect(text("Linux")).not.toMatch(/Console\.app|Get-Content/);
		expect(text("macOS")).not.toContain("stderr.log");
	});

	test("a platform it cannot tell gets the paths, not a command", () => {
		expect(text(undefined)).toContain("desktop/app.log");
		expect(text(undefined)).not.toMatch(/tail -n|Get-Content/);
	});

	test("the OS comes from the diagnostics when the context has none", () => {
		const r = withContext(
			{ message: "Failed to fetch", stack: "", where: "x" },
			{ diagnostics: linux(), breadcrumbs: [] },
		);
		expect(causeHint(r).detail).toContain("tail -n 50 ~/.atlas/desktop/stderr.log");
	});

	test("the bug form does not send a Linux or Windows reporter to a debug-only command", () => {
		const form = readFileSync(
			new URL("../../../../../.github/ISSUE_TEMPLATE/bug_report.yml", import.meta.url),
			"utf8",
		);
		expect(form).not.toContain("atlas desktop debug");
	});
});
