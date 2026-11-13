// Runs the real site/public/install.sh in a sandbox: a stub `curl` that serves a local release (feed,
// redirect, tarball, checksum), a `uname` shim so it behaves the same on any CI host, and a stand-in
// `atlas` inside the tarball that records what the installer runs. Nothing touches the network, the
// real HOME or a real release. The pure tag selection has its own vectors (test-bootstrap-channel.sh);
// this covers what an installer must never get wrong: verify before running, fail closed, and not
// strand a machine when the step after the install cannot reach the network.
import { afterEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const INSTALL_SH = join(root, "site/public/install.sh");
const TAG = "v9.9.9";
const TARGET = "x86_64-unknown-linux-gnu"; // what the `uname` shim reports

const sandboxes: string[] = [];
afterEach(() => {
	for (const d of sandboxes.splice(0)) rmSync(d, { recursive: true, force: true });
});

function script(path: string, body: string) {
	mkdirSync(join(path, ".."), { recursive: true });
	writeFileSync(path, `#!/bin/sh\n${body}\n`);
	chmodSync(path, 0o755);
}

function sh(cmd: string[], cwd: string): string {
	const p = Bun.spawnSync(cmd, { cwd });
	if (p.exitCode !== 0) throw new Error(`${cmd.join(" ")}: ${p.stderr.toString()}`);
	return p.stdout.toString();
}

interface Opts {
	/** The checksum file's content; default: the real digest of the tarball. */
	checksum?: (digest: string) => string;
	/** The fake `atlas update` exits with this (an offline machine: 1). */
	updateExit?: number;
	env?: Record<string, string>;
}

/** Build the sandbox and run install.sh. */
function install(opts: Opts = {}) {
	const root_ = mkdtempSync(join(tmpdir(), "atlas-install-sh-"));
	sandboxes.push(root_);
	const home = join(root_, "home");
	const stubs = join(root_, "stubs");
	const assets = join(root_, "assets");
	const calls = join(root_, "calls.log");
	mkdirSync(home, { recursive: true });
	mkdirSync(assets, { recursive: true });
	writeFileSync(calls, "");

	// The "atlas" in the tarball: `install` puts a copy at ~/.atlas/bin/atlas (what the real one does),
	// and every call is recorded.
	const atlas = join(root_, "pkg", "atlas");
	script(
		atlas,
		`echo "atlas $* [channel=\${ATLAS_UPDATE_CHANNEL:-}]" >> '${calls}'
if [ "$1" = install ]; then mkdir -p "$HOME/.atlas/bin" && cp "$0" "$HOME/.atlas/bin/atlas"; fi
if [ "$1" = update ]; then exit ${opts.updateExit ?? 0}; fi
exit 0`,
	);
	const version = TAG.slice(1);
	const asset = `atlas-${version}-${TARGET}.tar.gz`;
	sh(["tar", "-czf", join(assets, asset), "-C", join(root_, "pkg"), "atlas"], root_);
	const digest = createHash("sha256").update(readFileSync(join(assets, asset))).digest("hex");
	writeFileSync(join(assets, `${asset}.sha256`), (opts.checksum ?? ((d) => `${d}  ${asset}\n`))(digest));

	// curl: the releases/latest redirect, the Atom feed, HEAD probes and downloads from `assets/`.
	script(
		join(stubs, "curl"),
		`for a in "$@"; do url="$a"; done
out=""; prev=""; head=""
for a in "$@"; do
  [ "$prev" = "-o" ] && out="$a"
  case "$a" in -*I*) head=1 ;; esac
  prev="$a"
done
case "$url" in
  */releases/latest) printf 'HTTP/2 302\\r\\nlocation: https://github.com/codeatlasdev/atlas/releases/tag/${TAG}\\r\\n\\r\\n' ;;
  */releases.atom*) printf '<feed><entry><link href="https://github.com/codeatlasdev/atlas/releases/tag/${TAG}"/></entry></feed>' ;;
  */releases/download/*)
    f='${assets}'/"\${url##*/}"
    [ -f "$f" ] || exit 22
    [ -n "$head" ] && exit 0
    if [ -n "$out" ] && [ "$out" != /dev/null ]; then cp "$f" "$out"; else cat "$f"; fi ;;
  *) exit 22 ;;
esac`,
	);
	script(join(stubs, "uname"), `case "$1" in -s) echo Linux ;; -m) echo x86_64 ;; *) echo Linux ;; esac`);

	const p = Bun.spawnSync(["sh", INSTALL_SH], {
		cwd: root_,
		env: {
			PATH: `${stubs}:${process.env.PATH}`,
			HOME: home,
			TMPDIR: root_,
			...opts.env,
		},
	});
	return {
		code: p.exitCode,
		out: p.stdout.toString(),
		err: p.stderr.toString(),
		calls: readFileSync(calls, "utf8").trim().split("\n").filter(Boolean),
		installed: existsSync(join(home, ".atlas/bin/atlas")),
		// Temp dirs `mktemp -d` made in TMPDIR that the script must have removed again.
		leftovers: readdirSync(root_).filter((n) => n.startsWith("tmp.")),
	};
}

describe("install.sh", () => {
	test("verifies the checksum, runs `atlas install`, then moves to the default channel", () => {
		const r = install();
		expect(r.code).toBe(0);
		expect(r.out).toContain("sha256 verified");
		expect(r.calls[0]).toStartWith("atlas install");
		// No ATLAS_CHANNEL: the default channel is followed for this run only (never saved).
		expect(r.calls[1]).toMatch(/^atlas update \[channel=alpha\]$/);
		expect(r.installed).toBe(true);
	});

	// Incident class for any installer: a tarball that does not match its checksum must never run.
	test("a checksum mismatch aborts before anything is run or installed", () => {
		const r = install({ checksum: (d) => `${"0".repeat(d.length)}  x\n` });
		expect(r.code).not.toBe(0);
		expect(r.err).toContain("sha256 mismatch");
		expect(r.calls).toEqual([]);
		expect(r.installed).toBe(false);
	});

	test.each([
		["an empty checksum file", () => ""],
		["an HTML error page saved as the checksum", () => "<!doctype html><title>404</title>\n"],
		["a truncated digest", (d: string) => `${d.slice(0, 32)}\n`],
	])("%s is refused (fail closed)", (_name, checksum) => {
		const r = install({ checksum });
		expect(r.code).not.toBe(0);
		expect(r.calls).toEqual([]);
		expect(r.installed).toBe(false);
	});

	test("an uppercase digest is not accepted by accident of case", () => {
		// The release workflow writes lowercase (sha256sum); anything else is not what it published.
		const r = install({ checksum: (d) => `${d.toUpperCase()}  x\n` });
		expect(r.code).not.toBe(0);
		expect(r.calls).toEqual([]);
	});

	// Stranding guard: the installer put atlas in place; the step after it needs the network.
	test("an offline channel step leaves the install done and says how to finish", () => {
		const r = install({ updateExit: 1 });
		expect(r.code).toBe(0);
		expect(r.installed).toBe(true);
		expect(r.out).toContain("could not update now; run later: atlas update");
	});

	test("ATLAS_CHANNEL is saved with --channel (stable included)", () => {
		for (const ch of ["stable", "beta", "alpha"]) {
			const r = install({ env: { ATLAS_CHANNEL: ch } });
			expect(r.code).toBe(0);
			expect(r.calls[1]).toStartWith(`atlas update --channel ${ch}`);
		}
	});

	test("an unknown ATLAS_CHANNEL is refused before any download", () => {
		const r = install({ env: { ATLAS_CHANNEL: "nightly" } });
		expect(r.code).not.toBe(0);
		expect(r.err).toContain("unknown ATLAS_CHANNEL");
		expect(r.calls).toEqual([]);
	});

	test("no temp directory is left behind, on success or failure", () => {
		expect(install().leftovers).toEqual([]);
		expect(install({ checksum: () => "bad\n" }).leftovers).toEqual([]);
	});
});
