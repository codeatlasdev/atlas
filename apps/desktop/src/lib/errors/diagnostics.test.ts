// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../__fixtures__/diagnostics.json";
import { classify, diagnosticsMarkdown, findings, hostErrorReport, parseDiagnostics } from "./diagnostics.ts";
import { detailsText, issueUrl, markdownReport, redactText } from "./report.ts";

/** The real serialization of `diag::Diagnostics` (written by `diag::tests`, never hand-made). */
const D = parseDiagnostics(fixture);

describe("parseDiagnostics: the Rust payload", () => {
	test("reads every key the Rust struct serializes", () => {
		expect(D.versions).toEqual({
			app: "1.5.0-alpha.5",
			cli: "1.5.0-alpha.5",
			hostd: "1.5.0-alpha.4",
			hostdPid: 8664,
			hostdCaps: ["pty_attach_raw", "upgrade", "lsp"],
		});
		expect(D.os).toBe("macOS 15.1 (aarch64)");
		expect(D.install).toEqual({
			runningBundle: "/Applications/Atlas.app",
			installedApp: "/Applications/Atlas.app",
			cli: "~/.atlas/bin/atlas",
			cliLinkedTo: null,
			quarantined: true,
			translocated: false,
			channel: "alpha",
		});
		expect(D.hostdLog).toContain("listening on ~/.atlas/hostd.sock");
		expect(D.incidents[0]).toEqual({ at: 1791943000, kind: "unreachable", detail: "socket in use" });
		expect(D.lastCrash).toBe(
			"atlas 1.5.0-alpha.4: index out of bounds (crates/atlas-host/src/server.rs:9:1)",
		);
	});

	test("a payload that is not the struct degrades to empty fields instead of throwing", () => {
		for (const junk of [null, undefined, 3, "x", {}, { versions: 3, incidents: "no" }]) {
			const d = parseDiagnostics(junk);
			expect(d.versions.app).toBe("");
			expect(d.incidents).toEqual([]);
			expect(d.hostdLog).toBe("");
			expect(d.lastCrash).toBeNull();
			expect(d.install.quarantined).toBe(false);
			expect(d.install.runningBundle).toBeNull();
		}
	});
});

describe("redaction on the way out", () => {
	const leaky = {
		...fixture,
		hostdLog: "GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789 at /Users/bob/work",
		appLog: "Authorization: Bearer abc.def.ghi",
		// stderr.log carries whatever GTK/WebKit or a child printed, env dumps included (#273).
		stderrLog: "WebKitWebProcess: OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345 failed",
	};

	test("a report never contains a secret even if the log still did", () => {
		const r = hostErrorReport("Terminal host not reachable", parseDiagnostics(leaky));
		const md = markdownReport(r, 100_000);
		const url = decodeURIComponent(issueUrl(r));
		for (const out of [md, url]) {
			expect(out).not.toContain("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
			expect(out).not.toContain("abc.def.ghi");
			expect(out).not.toContain("/Users/bob");
			expect(out).not.toContain("sk-proj-abcdefghijklmnopqrstuvwxyz012345");
		}
	});

	// #265: the line that explained a blank window only existed on a terminal; now it is in the report.
	test("the stderr tail reaches the report", () => {
		const md = markdownReport(hostErrorReport("x", parseDiagnostics(fixture)), 100_000);
		expect(md).toContain("## stderr.log (tail)");
		expect(md).toContain("EGL_BAD_PARAMETER");
	});

	// The default limit (4000) cuts the tail of the report: stderr must come before the hostd/app tails.
	test("the stderr tail survives the default limit behind full hostd and app tails", () => {
		const lines = (tag) => Array.from({ length: 40 }, (_, i) => `${tag} line ${i} some ordinary log text`).join("\n");
		const d = parseDiagnostics({
			...fixture,
			hostdLog: lines("hostd"),
			appLog: lines("app"),
			stderrLog: `${lines("stderr")}\nCould not create default EGL display: EGL_BAD_PARAMETER`,
		});
		const md = markdownReport(hostErrorReport("x", d));
		expect(md).toContain("## stderr.log (tail)");
		expect(md).toContain("EGL_BAD_PARAMETER");
	});

	test("Copy details carries the redacted stderr tail", () => {
		const d = parseDiagnostics({
			...fixture,
			stderrLog: "WebKitWebProcess: OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345 EGL_BAD_PARAMETER",
		});
		const t = detailsText(hostErrorReport("x", d));
		expect(t).toContain("stderr.log (tail):");
		expect(t).toContain("EGL_BAD_PARAMETER");
		expect(t).not.toContain("sk-proj-abcdefghijklmnopqrstuvwxyz012345");
	});

	test("redactText keeps ordinary text and drops named secrets", () => {
		expect(redactText("terminal 3 exited with code 0")).toBe("terminal 3 exited with code 0");
		expect(redactText("password=hunter2 user=ana")).toBe("password=[redacted] user=ana");
		expect(redactText("see https://ana:pw@github.com/x.git")).toBe("see https://[redacted]@github.com/x.git");
	});
});

describe("the host error report", () => {
	const r = hostErrorReport("Terminal host not reachable. Use Restart terminal host.", D);

	test("carries versions, OS and the log tail into Copy as Markdown", () => {
		const md = markdownReport(r, 100_000);
		expect(md).toContain("Terminal host: 1.5.0-alpha.4");
		expect(md).toContain("CLI: 1.5.0-alpha.5");
		expect(md).toContain("macOS 15.1 (aarch64)");
		expect(md).toContain("## hostd.log (tail)");
		expect(md).toContain("Last crash: atlas 1.5.0-alpha.4: index out of bounds");
		expect(md).toContain("GITHUB_TOKEN=[redacted]");
	});

	test("the diagnosis names the skew between app, CLI and host", () => {
		expect(diagnosticsMarkdown(D)).toContain("Terminal host 1.5.0-alpha.4 is older than Atlas 1.5.0-alpha.5");
		const same = parseDiagnostics({ ...fixture, versions: { ...fixture.versions, hostd: "1.5.0-alpha.5" } });
		expect(diagnosticsMarkdown(same)).not.toContain("older than");
	});

	test("the issue URL fills the bug_report.yml fields and stays under GitHub's limit", () => {
		const url = issueUrl(r);
		expect(url.length).toBeLessThan(8000);
		const q = new URL(url).searchParams;
		expect(q.get("template")).toBe("bug_report.yml");
		// bug_report.yml ids: what, repro, surface, channel, version, os, logs.
		expect(q.get("surface")).toBe("Atlas Desktop");
		expect(q.get("version")).toBe("atlas 1.5.0-alpha.5");
		expect(q.get("os")).toBe("macOS 15.1 (aarch64)");
		expect(q.get("what")).toContain("Terminal host not reachable");
		expect(q.get("logs")).toContain("listening on ~/.atlas/hostd.sock");
		expect(q.get("logs")).not.toContain("ghp_");
	});

	test("a log far over the budget is cut, never the URL", () => {
		const big = parseDiagnostics({ ...fixture, hostdLog: "line\n".repeat(5000) });
		const url = issueUrl(hostErrorReport("x", big));
		expect(url.length).toBeLessThan(8000);
	});
});

describe("diagnose: what a remote maintainer reads first", () => {
	const w = (over) => parseDiagnostics({ ...fixture, ...over });
	test("a clean machine has no finding", () => {
		const same = w({ versions: { ...fixture.versions, hostd: "1.5.0-alpha.5" } });
		expect(findings(same)).toEqual([]);
	});
	test("host older than the app, CLI older than the app, translocation, dev link are each named", () => {
		expect(findings(D).join(" ")).toContain("Terminal host 1.5.0-alpha.4 is older than Atlas 1.5.0-alpha.5");
		const cli = w({ versions: { ...fixture.versions, cli: "1.5.0-alpha.4", hostd: "1.5.0-alpha.5" } });
		expect(findings(cli).join(" ")).toContain("CLI 1.5.0-alpha.4 differs from the app 1.5.0-alpha.5");
		const t = w({ install: { ...fixture.install, translocated: true } });
		expect(findings(t).join(" ")).toContain("App Translocation");
		const l = w({ install: { ...fixture.install, cliLinkedTo: "/repo/target/debug/atlas" } });
		expect(findings(l).join(" ")).toContain("linked to a dev build");
	});
	test("Markdown and Copy details carry the install block and the findings", () => {
		const r = hostErrorReport("x", w({ install: { ...fixture.install, translocated: true } }));
		const md = markdownReport(r, 100_000);
		expect(md).toContain("## Install");
		expect(md).toContain("- Running bundle: /Applications/Atlas.app");
		expect(md).toContain("- CLI: ~/.atlas/bin/atlas");
		expect(md).toContain("App Translocation");
		expect(detailsText(r)).toContain("Installed app: /Applications/Atlas.app");
	});
});

describe("Copy details = Copy diagnostics", () => {
	test("detailsText carries versions, OS and the redacted log tail", () => {
		const t = detailsText(hostErrorReport("Terminal host not reachable", D));
		expect(t).toContain("Terminal host: 1.5.0-alpha.4");
		expect(t).toContain("CLI: 1.5.0-alpha.5");
		expect(t).toContain("macOS 15.1 (aarch64)");
		expect(t).toContain("GITHUB_TOKEN=[redacted]");
		expect(t).not.toContain("ghp_");
	});
});

describe("classify: which host failures get a Restart terminal host action", () => {
	test("the Rust 'not reachable' error and a dropped connection do", () => {
		expect(classify("Terminal host not reachable. Terminals cannot start...")).toBe("host");
		expect(classify("hostd disconnected")).toBe("host");
		expect(classify("connect /Users/x/.atlas/hostd.sock: No such file or directory")).toBe("host");
		expect(classify("hostd: unauthorized")).toBe("host");
	});
	test("ordinary errors do not", () => {
		expect(classify("cwd does not exist")).toBe("other");
		expect(classify("unknown pty abc")).toBe("other");
	});
});
