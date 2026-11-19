// scripts/ci-apt.sh, the CI's apt install (#604): from the cache without asking a mirror, and with a time limit and one retry
// when it has to. `sudo`, `apt-get` and `dpkg` are shims that log every call; the verdicts (what ran, what was cached, the
// exit status) are the script's. "Slow" is a shim that hangs, and the limits are seconds here, not minutes.
import { afterEach, describe, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dir, "ci-apt.sh");
const dirs: string[] = [];
afterEach(() => {
	for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

const APT_GET = `#!/bin/sh
echo "apt-get $*" >> "$LOG"
# The fast mirror is the one that comes with its own sources directory.
[ -z "$HANG_FAST" ] || case "$*" in *SourceParts=*) exec sleep 60 ;; esac
[ -z "$FAIL_UPDATE" ] || case "$*" in update*) exit 100 ;; esac
case "$*" in
*--download-only*)
  # The Nth download call hangs while N <= HANG_DOWNLOADS: a mirror that gives a few bytes a minute.
  n=$(grep -c -e '--download-only' "$LOG")
  if [ "$n" -le "\${HANG_DOWNLOADS:-0}" ]; then exec sleep 60; fi
  : > "$ARCHIVES/libfoo_1_amd64.deb"
  : > "$ARCHIVES/libbar_1_amd64.deb"
  ;;
esac
`;

/** A runner: shims on PATH, an apt archive directory that already holds one package of the image, a cache dir. */
function runner() {
	const d = mkdtempSync(join(tmpdir(), "ci-apt-"));
	dirs.push(d);
	const bin = join(d, "bin");
	const archives = join(d, "archives");
	const cache = join(d, "cache");
	mkdirSync(bin);
	mkdirSync(archives);
	const log = join(d, "log");
	writeFileSync(log, "");
	writeFileSync(join(archives, "from-the-image_1_amd64.deb"), "");
	writeFileSync(join(d, "os-release"), "NAME=Ubuntu\nVERSION_CODENAME=resolute\n");
	const shim = (name: string, body: string) => {
		writeFileSync(join(bin, name), body);
		chmodSync(join(bin, name), 0o755);
	};
	shim("sudo", '#!/bin/sh\nexec "$@"\n');
	shim("apt-get", APT_GET);
	shim("dpkg", `#!/bin/sh\necho "dpkg $*" >> "$LOG"\n[ -z "$DPKG_FAILS" ]\n`);
	const run = (env: Record<string, string> = {}, ...packages: string[]) => {
		const p = Bun.spawnSync(["bash", script, cache, ...(packages.length ? packages : ["ffmpeg", "sway"])], {
			env: {
				...process.env,
				PATH: `${bin}:${process.env.PATH}`,
				LOG: log,
				ARCHIVES: archives,
				CI_APT_ARCHIVES: archives,
				CI_APT_MIRROR: "", // only the image's own sources; the fast mirror has its tests below
				CI_APT_UPDATE_SECS: "30",
				CI_APT_DOWNLOAD_SECS: "1",
				CI_APT_INSTALL_SECS: "30",
				...env,
			},
		});
		return { code: p.exitCode, out: p.stdout.toString() + p.stderr.toString() };
	};
	// The fast mirror on: an x86_64 Ubuntu whose os-release names a codename.
	const fast = { CI_APT_MIRROR: "http://fast.test/ubuntu/", CI_APT_ARCH: "x86_64", CI_APT_OS_RELEASE: join(d, "os-release") };
	const calls = () => readFileSync(log, "utf8").split("\n").filter(Boolean);
	return { run, calls, cache, archives, fast };
}

describe.skipIf(process.platform === "win32")("ci-apt.sh", () => {
	test("a cold cache installs from the mirror and caches what the image did not have", () => {
		const r = runner();
		const res = r.run();
		expect(res.code).toBe(0);
		const calls = r.calls();
		expect(calls.map((c) => c.split(" ").slice(0, 2).join(" "))).toEqual(["apt-get update", "apt-get install", "apt-get install"]);
		expect(calls[1]).toContain("--download-only");
		expect(calls[2]).not.toContain("--download-only");
		// The two downloaded files, not the one the image already had, and the package list the cache is keyed by.
		expect(readdirSync(r.cache).sort()).toEqual(["libbar_1_amd64.deb", "libfoo_1_amd64.deb", "packages.txt"]);
		expect(readFileSync(join(r.cache, "packages.txt"), "utf8")).toBe("ffmpeg\nsway\n");
	});

	test("a warm cache installs with dpkg and never asks the mirror", () => {
		const r = runner();
		expect(r.run().code).toBe(0);
		const warm = r.calls().length;
		const res = r.run();
		expect(res.code).toBe(0);
		const after = r.calls().slice(warm);
		expect(after).toHaveLength(1);
		expect(after[0]).toMatch(/^dpkg -i .*libbar_1_amd64\.deb .*libfoo_1_amd64\.deb$/);
		expect(res.out).toContain("from the cache");
	});

	test("a cache whose packages do not install is dropped and the mirror is used, which fills the cache again", () => {
		const r = runner();
		expect(r.run().code).toBe(0);
		// A new runner: the first run's downloads are not in its apt archives.
		for (const f of ["libfoo_1_amd64.deb", "libbar_1_amd64.deb"]) rmSync(join(r.archives, f));
		const res = r.run({ DPKG_FAILS: "1" });
		expect(res.code).toBe(0);
		expect(res.out).toContain("did not install");
		expect(r.calls().filter((c) => c.startsWith("apt-get update"))).toHaveLength(2);
		expect(readdirSync(r.cache).sort()).toEqual(["libbar_1_amd64.deb", "libfoo_1_amd64.deb", "packages.txt"]);
	});

	// Incident (#604): a mirror that crawled held the job for 14 min, then 25+ min. A stalled download is ended and retried once.
	test("a download that stalls is ended at its limit and tried once more, which finishes the job", () => {
		const r = runner();
		const res = r.run({ HANG_DOWNLOADS: "1" });
		expect(res.code).toBe(0);
		expect(r.calls().filter((c) => c.includes("--download-only"))).toHaveLength(2);
		expect(res.out).toContain("the download failed or took longer than 1s");
		expect(existsSync(join(r.cache, "packages.txt"))).toBe(true);
	});

	test("a mirror that stalls on both rounds fails the step: no third try, no install, nothing cached", () => {
		const r = runner();
		const res = r.run({ HANG_DOWNLOADS: "9" });
		expect(res.code).not.toBe(0);
		expect(r.calls().filter((c) => c.includes("--download-only"))).toHaveLength(2);
		expect(r.calls().filter((c) => c.startsWith("apt-get install") && !c.includes("--download-only"))).toEqual([]);
		expect(res.out).toContain("::error::ci-apt: the apt mirror is too slow or down (two rounds ran out of time); rerun the job");
		expect(existsSync(join(r.cache, "packages.txt"))).toBe(false);
	});

	// Incident (#604, 2026-11-19): the image's list picked us-east-2.ec2.archive.ubuntu.com at ~7 kB/s while azure.archive.ubuntu.com
	// gave 30-85 MB/s from the same runner.
	test("the fast mirror comes first, with its own sources, and the install reads the same lists", () => {
		const r = runner();
		const res = r.run(r.fast);
		expect(res.code).toBe(0);
		const calls = r.calls();
		expect(calls).toHaveLength(3);
		for (const c of calls) expect(c).toMatch(/Dir::Etc::SourceList=\/dev\/null -o Dir::Etc::SourceParts=\S+/);
		expect(res.out).toContain("installed from http://fast.test/ubuntu/");
	});

	test("a fast mirror that stalls is left after its limit for the image's own sources, which finish the job", () => {
		const r = runner();
		const res = r.run({ ...r.fast, HANG_FAST: "1", CI_APT_UPDATE_SECS: "1" });
		expect(res.code).toBe(0);
		const calls = r.calls();
		expect(calls[0]).toContain("SourceParts=");
		expect(calls.slice(1).every((c) => !c.includes("SourceParts="))).toBe(true);
		expect(calls.filter((c) => c.startsWith("apt-get update"))).toHaveLength(2);
		expect(res.out).toContain("apt-get update failed or took longer than 1s (http://fast.test/ubuntu/)");
		expect(res.out).toContain("installed from the image sources");
	});

	test("an architecture the fast mirror does not serve (arm64 is on ports) never sees it", () => {
		const r = runner();
		expect(r.run({ ...r.fast, CI_APT_ARCH: "aarch64" }).code).toBe(0);
		expect(r.calls().some((c) => c.includes("SourceParts="))).toBe(false);
	});

	// A package that does not exist fails in a second, twice: the mirror says nothing, and the step must not claim it did.
	test("a command that fails by itself is not blamed on the mirror", () => {
		const r = runner();
		const res = r.run({ FAIL_UPDATE: "1" });
		expect(res.code).not.toBe(0);
		expect(res.out).toContain("apt-get update failed or took longer");
		expect(res.out).not.toContain("::error::");
	});

	test("no packages is a usage error, not an install of nothing", () => {
		const r = runner();
		const p = Bun.spawnSync(["bash", script, r.cache], { env: process.env });
		expect(p.exitCode).toBe(2);
	});
});
