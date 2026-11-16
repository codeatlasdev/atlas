// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import linux from "../__fixtures__/diagnostics-linux.json";
import mac from "../__fixtures__/diagnostics.json";
import { parseDiagnostics } from "./diagnostics.ts";
import { machineLines } from "./machine.ts";
import { detailsText, issueUrl, markdownReport } from "./report.ts";

/** What `atlas doctor --bundle` prints for the same value: written by `bundle::tests` in Rust. */
const rust = (name: string) =>
	readFileSync(new URL(`../__fixtures__/${name}.details.txt`, import.meta.url), "utf8").trimEnd();

/** The issue URL's text as GitHub shows it in the form: `+` is a space. */
const urlOf = (r) => decodeURIComponent(issueUrl(r).replace(/\+/g, " "));

const reportOf = (fixture) => {
	const d = parseDiagnostics(fixture);
	return {
		message: "Terminal host not reachable",
		stack: "",
		where: "terminal host",
		kind: "hostd",
		version: d.versions.app,
		context: { diagnostics: d, breadcrumbs: [] },
	};
};

/** The block both renderers own: from "Terminal host:" to the first blank line (tails follow it). */
const block = (text: string) => {
	const lines = text.split("\n");
	const from = lines.indexOf(lines.find((l) => l.startsWith("Terminal host:")));
	const rest = lines.slice(from);
	const end = rest.indexOf("");
	return end < 0 ? rest : rest.slice(0, end);
};

/** One `name (tail):` section of a details text. */
const tailOf = (text: string, name: string) => {
	const lines = text.split("\n");
	const i = lines.indexOf(`${name} (tail):`);
	if (i < 0) return null;
	const end = lines.indexOf("", i);
	return lines.slice(i, end < 0 ? undefined : end).join("\n");
};

describe("Copy details and atlas doctor --bundle print the same report", () => {
	for (const [name, fixture] of [
		["diagnostics", mac],
		["diagnostics-linux", linux],
	]) {
		test(`${name}: head, shared block and log tails are identical`, () => {
			const ts = detailsText(reportOf(fixture));
			const rs = rust(name);
			expect(ts.split("\n").slice(0, 2)).toEqual(rs.split("\n").slice(0, 2));
			expect(block(ts)).toEqual(block(rs));
			for (const log of ["stderr.log", "hostd.log"]) {
				expect(tailOf(ts, log)).toBe(tailOf(rs, log));
			}
		});
	}

	test("the Linux report carries distro, session, GPU, packaging and crash lines", () => {
		const b = block(detailsText(reportOf(linux)));
		for (const line of [
			"Distro: Arch Linux [arch]",
			"Session: wayland, desktop Hyprland, compositor Hyprland",
			"GPU: AMD (amdgpu)",
			"Web view: WebKitGTK 2.54.1",
			"AppImage: ~/Apps/Atlas_1.5.0-alpha.25_amd64.AppImage",
			"Core dumps (24h): 1",
			"Atlas crash reports: newest 2023-11-14 22:13:20 UTC (2h ago)",
		]) {
			expect(b).toContain(line);
		}
	});
});

describe("machineLines", () => {
	test("reads the keys Rust serializes and prints the lines Rust expects", () => {
		expect(machineLines(parseDiagnostics(linux))).toEqual([
			"Distro: Arch Linux [arch]",
			"Kernel: 7.2.15-1-omarchy",
			"Session: wayland, desktop Hyprland, compositor Hyprland",
			"GPU: AMD (amdgpu)",
			"Mesa: 26.2.5-1",
			"GTK: 3.24.52",
			"Web view: WebKitGTK 2.54.1",
			"Graphics env: WEBKIT_DISABLE_DMABUF_RENDERER=1",
			"AppImage: ~/Apps/Atlas_1.5.0-alpha.25_amd64.AppImage",
			"Binary: /tmp/.mount_AtlasAbc/usr/bin/atlas-desktop",
			"~/.local/bin: atlas",
			"Core dumps (24h): 1",
			"  2023-11-14 22:15:00 UTC SIGSEGV pid 4242 /tmp/.mount_AtlasAbc/usr/lib/webkit2gtk-4.1/WebKitWebProcess",
			"Atlas crash reports: newest 2023-11-14 22:13:20 UTC (2h ago)",
		]);
	});

	test("a payload without the new keys (an older app) degrades to the one line that always applies", () => {
		const { platform, packaging, crashes, ...old } = mac;
		expect(machineLines(parseDiagnostics(old))).toEqual(["Atlas crash reports: none"]);
		expect(machineLines(parseDiagnostics(null))).toEqual(["Atlas crash reports: none"]);
	});

	test("a core dump query that failed says unavailable, one that does not apply says nothing", () => {
		const d = parseDiagnostics(linux);
		d.crashes.coredumps = [];
		d.crashes.coredumpStatus = "unavailable";
		expect(machineLines(d)).toContain("Core dumps (24h): unavailable");
		d.crashes.coredumpStatus = "";
		expect(machineLines(d).some((l) => l.startsWith("Core dumps"))).toBe(false);
	});
});

describe("the new fields on the way out", () => {
	// Rust redacts tokens (`bundle::tests`); the page drops homes and query secrets again, and must not
	// mistake a real WebKit variable or a mount path for a secret.
	const leaky = {
		...linux,
		packaging: { ...linux.packaging, appimage: "/Users/bob/Apps/Atlas.AppImage?token=abcdef123456" },
	};

	test("details, the markdown report and the issue URL carry no home path or query secret", () => {
		const r = reportOf(leaky);
		for (const out of [detailsText(r), markdownReport(r), urlOf(r)]) {
			expect(out).not.toContain("/Users/bob");
			expect(out).not.toContain("abcdef123456");
			expect(out).toContain("~/Apps/Atlas.AppImage");
		}
	});

	test("WEBKIT_DISABLE_DMABUF_RENDERER and the AppImage mount path survive as written", () => {
		const r = reportOf(linux);
		for (const out of [detailsText(r), markdownReport(r), urlOf(r)]) {
			expect(out).toContain("WEBKIT_DISABLE_DMABUF_RENDERER=1");
			expect(out).toContain("/tmp/.mount_AtlasAbc/usr/lib/webkit2gtk-4.1/WebKitWebProcess");
		}
	});

	// The machine lines are the only text `scrub` (not `redactText`) touches: a log tail next to them must
	// still go through the full redaction on every path out.
	test("with the machine block present, a stderr tail with a token and a home path is redacted everywhere", () => {
		const r = reportOf({
			...linux,
			stderrLog:
				"WebKitWebProcess: GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789 failed in /Users/bob/work/app",
			hostdLog: "spawn failed: OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz012345",
		});
		for (const out of [detailsText(r), markdownReport(r), urlOf(r)]) {
			expect(out).toContain("GPU: AMD (amdgpu)");
			expect(out).not.toContain("ghp_abcdefghijklmnopqrstuvwxyz0123456789");
			expect(out).not.toContain("sk-proj-abcdefghijklmnopqrstuvwxyz012345");
			expect(out).not.toContain("/Users/bob");
		}
	});

	test("the markdown report lists the machine facts in the Environment", () => {
		const md = markdownReport(reportOf(linux));
		expect(md).toContain("- GPU: AMD (amdgpu)");
		expect(md).toContain("- Session: wayland, desktop Hyprland, compositor Hyprland");
	});
});

// #274 budget: the whole URL stays under GitHub's limit (8000) with the machine block in the never-cut Environment.
describe("the machine block and the URL budget", () => {
	const tail = (tag) =>
		Array.from(
			{ length: 40 },
			(_, i) =>
				`2026-11-15T10:${String(i).padStart(2, "0")}:03Z ${tag} WARN atlas_desktop::host: line ${i} retrying connection to ~/.atlas/hostd.sock`,
		).join("\n");
	const stack = Array.from(
		{ length: 40 },
		(_, i) => `    at renderComponentLongName${i} (SomeDeeplyNestedComponent${i}.svelte:${i + 1}:13)`,
	).join("\n");

	test("stays under 8000 with the full Linux machine block, three full tails and 40 frames, and keeps the block", () => {
		const r = {
			...reportOf({ ...linux, hostdLog: tail("hostd"), appLog: tail("app"), stderrLog: tail("stderr") }),
			stack,
		};
		const url = issueUrl(r);
		expect(url.length).toBeLessThan(8000);
		const logs = new URL(url).searchParams.get("logs");
		for (const line of machineLines(parseDiagnostics(linux))) expect(logs).toContain(`- ${line}`);
	});
});
