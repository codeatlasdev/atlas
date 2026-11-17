// Runs the real site/public/install.ps1 the way a user does (`irm <url> | iex`) under Windows PowerShell 5.1
// (powershell.exe, what a stock Windows has) and PowerShell 7 (pwsh), against a local server that plays
// GitHub (ATLAS_GITHUB_ORIGIN, the script's test hook): the releases/latest redirect, the Atom feed, HEAD
// probes and the downloads. The real curl.exe and tar.exe run; the `atlas.exe` in the tarball is a tiny
// stand-in (compiled with the .NET Framework's csc.exe) that records what the installer runs. Nothing
// touches GitHub, the real profile or PATH. Windows only: the script needs curl.exe and tar.exe.
//
// Why 5.1 matters (#519): there, a native command whose stderr is redirected (`2>$null`) turns every
// stderr line into an error record, and with $ErrorActionPreference = 'Stop' the expected 404 probe of a
// Stable that has no Windows build ended the whole installer instead of falling back to the alpha.
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const INSTALL_PS1 = join(root, "site/public/install.ps1");
const TARGET = "x86_64-pc-windows-msvc";
const STABLE = "v9.8.0";
const ALPHA = "v9.9.9-alpha.1";

// The script is piped through `irm` (no BOM, charset utf-8 in site/public/_headers): non-ASCII text in it
// would be decoded as Windows-1252 by a client that ignores the header and print as mojibake. Any OS.
test("install.ps1 is plain ASCII", () => {
	const bytes = readFileSync(INSTALL_PS1);
	const bad = [...bytes].findIndex((b) => b > 0x7f);
	expect(bad).toBe(-1);
});

const shells = [
	{ name: "Windows PowerShell 5.1", exe: Bun.which("powershell") },
	{ name: "PowerShell 7", exe: Bun.which("pwsh") },
];

const FAKE_ATLAS = `using System;
using System.IO;
class P {
  static int Main(string[] a) {
    string calls = Environment.GetEnvironmentVariable("ATLAS_TEST_CALLS");
    string ch = Environment.GetEnvironmentVariable("ATLAS_UPDATE_CHANNEL") ?? "";
    File.AppendAllText(calls, "atlas " + string.Join(" ", a) + " [channel=" + ch + "]\\n");
    if (a.Length > 0 && a[0] == "install") {
      string bin = Path.Combine(Environment.GetEnvironmentVariable("USERPROFILE"), ".atlas", "bin");
      Directory.CreateDirectory(bin);
      File.Copy(typeof(P).Assembly.Location, Path.Combine(bin, "atlas.exe"), true);
    }
    return 0;
  }
}
`;

let work = "";
let tarball: Buffer;
let digest = "";

beforeAll(() => {
	if (process.platform !== "win32") return;
	work = mkdtempSync(join(tmpdir(), "atlas-install-ps1-"));
	const pkg = join(work, "pkg");
	mkdirSync(pkg, { recursive: true });
	writeFileSync(join(pkg, "atlas.cs"), FAKE_ATLAS);
	const csc = join(process.env.windir ?? "C:\\Windows", "Microsoft.NET", "Framework64", "v4.0.30319", "csc.exe");
	const c = Bun.spawnSync([csc, "/nologo", `/out:${join(pkg, "atlas.exe")}`, join(pkg, "atlas.cs")]);
	if (c.exitCode !== 0) throw new Error(`csc: ${c.stdout.toString()}${c.stderr.toString()}`);
	// Windows' own bsdtar, never a Git Bash tar (it reads `C:` as a remote host).
	const tar = join(process.env.windir ?? "C:\\Windows", "System32", "tar.exe");
	const t = Bun.spawnSync([tar, "-czf", join(work, "atlas.tar.gz"), "-C", pkg, "atlas.exe"]);
	if (t.exitCode !== 0) throw new Error(`tar: ${t.stderr.toString()}`);
	tarball = readFileSync(join(work, "atlas.tar.gz"));
	digest = createHash("sha256").update(tarball).digest("hex");
	// csc.exe cold-starts the .NET compiler; on a busy runner that alone passed bun's 5 s hook default.
}, 120_000);
afterAll(() => {
	if (work) rmSync(work, { recursive: true, force: true });
});

interface Release {
	/** Tags in the Atom feed (newest first). */
	feed: string[];
	/** The tag the /releases/latest redirect points at. */
	latest?: string;
	/** Tags whose release has a Windows build; every other asset URL is a 404, like 1.4.0's. */
	withBuild: string[];
	checksum?: (digest: string) => string;
}

async function install(shell: string, rel: Release, env: Record<string, string> = {}) {
	const sandbox = mkdtempSync(join(work, "run-"));
	const home = join(sandbox, "home");
	const tmp = join(sandbox, "tmp");
	mkdirSync(home, { recursive: true });
	mkdirSync(tmp, { recursive: true });
	const calls = join(sandbox, "calls.log");
	writeFileSync(calls, "");

	const server = Bun.serve({
		hostname: "127.0.0.1",
		port: 0,
		fetch(req) {
			const url = new URL(req.url);
			const origin = url.origin;
			const p = url.pathname;
			if (p === "/install.ps1") {
				return new Response(readFileSync(INSTALL_PS1), { headers: { "content-type": "text/plain; charset=utf-8" } });
			}
			if (p === "/codeatlasdev/atlas/releases/latest") {
				if (!rel.latest) return new Response("not found", { status: 404 });
				return new Response(null, { status: 302, headers: { location: `${origin}/codeatlasdev/atlas/releases/tag/${rel.latest}` } });
			}
			if (p === "/codeatlasdev/atlas/releases.atom") {
				const entries = url.searchParams.has("after") ? [] : rel.feed;
				const links = entries.map((t) => `<entry><link href="${origin}/codeatlasdev/atlas/releases/tag/${t}"/></entry>`);
				return new Response(`<feed>${links.join("")}</feed>`, { headers: { "content-type": "application/atom+xml; charset=utf-8" } });
			}
			const m = p.match(/^\/codeatlasdev\/atlas\/releases\/download\/([^/]+)\/(atlas-.+)$/);
			if (m && rel.withBuild.includes(m[1])) {
				const asset = `atlas-${m[1].replace(/^v/, "")}-${TARGET}.tar.gz`;
				if (m[2] === asset) return new Response(tarball);
				if (m[2] === `${asset}.sha256`) return new Response((rel.checksum ?? ((d) => `${d}  ${asset}\n`))(digest));
			}
			return new Response("not found", { status: 404 });
		},
	});
	try {
		const drive = /^[A-Za-z]:/.test(home) ? home.slice(0, 2) : "";
		// The runner's step shell is pwsh, whose PSModulePath would make a child powershell.exe (5.1) miss its own
		// built-in modules (Get-FileHash): each shell gets the default it has on a user's machine.
		const { PSModulePath: _inherited, ...inherited } = process.env;
		const p = Bun.spawn(
			[shell, "-NoProfile", "-NonInteractive", "-Command", `irm http://127.0.0.1:${server.port}/install.ps1 | iex`],
			{
				cwd: sandbox,
				env: {
					...inherited,
					USERPROFILE: home,
					HOME: home,
					...(drive ? { HOMEDRIVE: drive, HOMEPATH: home.slice(2) } : {}),
					TEMP: tmp,
					TMP: tmp,
					ATLAS_TEST_CALLS: calls,
					ATLAS_GITHUB_ORIGIN: `http://127.0.0.1:${server.port}`,
					ATLAS_CHANNEL: "",
					...env,
				},
				stdout: "pipe",
				stderr: "pipe",
			},
		);
		const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
		return {
			code,
			out,
			err,
			calls: readFileSync(calls, "utf8").trim().split("\n").filter(Boolean),
			installed: existsSync(join(home, ".atlas", "bin", "atlas.exe")),
		};
	} finally {
		server.stop(true);
	}
}

/** A run that must have succeeded; when it did not, the failure shows everything the installer printed. */
function succeeded(r: { code: number | null; out: string; err: string }) {
	if (r.code !== 0) throw new Error(`install.ps1 exited ${r.code}\n--- stdout\n${r.out}\n--- stderr\n${r.err}`);
}

for (const { name, exe } of shells) describe.skipIf(process.platform !== "win32")(`install.ps1 in ${name}`, () => {
	const run = (rel: Release, env?: Record<string, string>) => {
		if (!exe) throw new Error("this shell is not installed on the runner");
		return install(exe, rel, env);
	};

	// #519: the newest Stable (1.4.0 in real life) has no Windows build, so its probe is a 404 that must
	// mean "try the next candidate", never end the script.
	test("a Stable without a Windows build falls back to the newest alpha and installs it", async () => {
		const r = await run({ feed: [ALPHA, STABLE], latest: ALPHA, withBuild: [ALPHA] });
		expect(r.err).not.toContain("NativeCommandError");
		expect(r.out).toContain(`no Stable build for ${TARGET} yet (newest Stable: ${STABLE}); starting from ${ALPHA}`);
		expect(r.out).toContain("sha256 verified");
		succeeded(r);
		expect(r.calls[0]).toStartWith("atlas install");
		// No ATLAS_CHANNEL: the default channel is followed for this run only (never saved).
		expect(r.calls[1]).toBe("atlas update [channel=alpha]");
		expect(r.installed).toBe(true);
	}, 120_000);

	test("a Stable with a Windows build is installed as it is, with no fallback message", async () => {
		const r = await run({ feed: [ALPHA, STABLE], latest: ALPHA, withBuild: [STABLE, ALPHA] });
		succeeded(r);
		expect(r.out).not.toContain("no Stable build");
		expect(r.out).toContain(`Atlas ${STABLE.slice(1)} (${TARGET})`);
		expect(r.installed).toBe(true);
	}, 120_000);

	test("on the stable channel, a Stable without a Windows build is refused with the way out", async () => {
		const r = await run({ feed: [ALPHA, STABLE], latest: ALPHA, withBuild: [ALPHA] }, { ATLAS_CHANNEL: "stable" });
		expect(r.code, `${r.out}${r.err}`).not.toBe(0);
		expect(r.out).toContain(`no Stable release has a build for ${TARGET} yet (newest Stable: ${STABLE})`);
		expect(r.err).not.toContain("NativeCommandError");
		expect(r.calls).toEqual([]);
		expect(r.installed).toBe(false);
	}, 120_000);

	test("no release with a Windows build at all says so", async () => {
		const r = await run({ feed: [STABLE], latest: ALPHA, withBuild: [] });
		expect(r.code, `${r.out}${r.err}`).not.toBe(0);
		expect(r.out).toContain(`could not find a release with a build for ${TARGET}`);
		expect(r.err).not.toContain("NativeCommandError");
		expect(r.calls).toEqual([]);
	}, 120_000);

	// Incident class for any installer: a tarball that does not match its checksum must never run.
	test("a checksum mismatch aborts before anything is run or installed", async () => {
		const r = await run({ feed: [STABLE], latest: STABLE, withBuild: [STABLE], checksum: (d) => `${"0".repeat(d.length)}  x\n` });
		expect(r.code, `${r.out}${r.err}`).not.toBe(0);
		expect(r.out).toContain("sha256 mismatch");
		expect(r.calls).toEqual([]);
		expect(r.installed).toBe(false);
	}, 120_000);

	test("a missing checksum file is a clear failure, not a PowerShell error record", async () => {
		const r = await run({ feed: [STABLE], latest: STABLE, withBuild: [STABLE], checksum: () => "" });
		expect(r.code, `${r.out}${r.err}`).not.toBe(0);
		expect(r.calls).toEqual([]);
		expect(r.installed).toBe(false);
	}, 120_000);

	test("ATLAS_CHANNEL is saved with --channel", async () => {
		const r = await run({ feed: [STABLE], latest: STABLE, withBuild: [STABLE] }, { ATLAS_CHANNEL: "stable" });
		succeeded(r);
		expect(r.calls[1]).toStartWith("atlas update --channel stable");
	}, 120_000);

	test("an unknown ATLAS_CHANNEL is refused before any download", async () => {
		const r = await run({ feed: [STABLE], latest: STABLE, withBuild: [STABLE] }, { ATLAS_CHANNEL: "nightly" });
		expect(r.code, `${r.out}${r.err}`).not.toBe(0);
		expect(r.out).toContain("unknown ATLAS_CHANNEL");
		expect(r.calls).toEqual([]);
	}, 120_000);
});
