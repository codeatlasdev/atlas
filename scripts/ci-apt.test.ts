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
# What apt prints and exits (100) when it cannot reach a mirror: not a timeout (143), a refused or timed out connection.
if [ -n "$NET_FAIL" ]; then case "$*" in
  update*) [ "$NET_FAIL" = download ] || { echo "Err:1 http://azure.archive.ubuntu.com/ubuntu resolute InRelease"; echo "  Could not connect to azure.archive.ubuntu.com:80 (1.2.3.4), connection timed out"; echo "E: Failed to fetch http://azure.archive.ubuntu.com/ubuntu/dists/resolute/InRelease"; exit 100; } ;;
  *--download-only*) echo "Err:1 http://x/ubuntu resolute/main amd64 libva2 amd64 2.23.0-1"; echo "  Connection failed [IP: 1.2.3.4 80]"; echo "E: Failed to fetch http://x/ubuntu/pool/libva2.deb"; exit 100 ;;
esac; fi
[ -z "$UNKNOWN_PACKAGE" ] || case "$*" in *--download-only*) echo "E: Unable to locate package nosuchpkg"; exit 100 ;; esac
case "$*" in
*--download-only*)
  # The Nth download call hangs while N <= HANG_DOWNLOADS: a mirror that gives a few bytes a minute.
  n=$(grep -c -e '--download-only' "$LOG")
  if [ "$n" -le "\${HANG_DOWNLOADS:-0}" ]; then exec sleep 60; fi
  : > "$ARCHIVES/libfoo_\${FOO:-1}_amd64.deb"
  : > "$ARCHIVES/libbar_1_amd64.deb"
  ;;
update*) ;;
*) # the install: it needs every file in the archives, and records what is installed now
  echo "libfoo \${FOO:-1}" >> "$INSTALLED"; echo "libbar 1" >> "$INSTALLED" ;;
esac
`;
const DPKG_QUERY = `#!/bin/sh
# dpkg-query -W -f='\${Version}' <name>: the last version apt-get "installed"
for a; do n=$a; done
grep "^$n " "$INSTALLED" | tail -1 | cut -d' ' -f2 | tr -d '\\n'
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
	shim("dpkg-query", DPKG_QUERY);
	shim("dpkg", `#!/bin/sh\necho "dpkg $*" >> "$LOG"\n[ -z "$DPKG_FAILS" ]\n`);
	const run = (env: Record<string, string> = {}, ...packages: string[]) => {
		const p = Bun.spawnSync(["bash", script, cache, ...(packages.length ? packages : ["ffmpeg", "sway"])], {
			env: {
				...process.env,
				PATH: `${bin}:${process.env.PATH}`,
				LOG: log,
				ARCHIVES: archives,
				INSTALLED: join(d, "installed"),
				CI_APT_IMAGE: "image-1",
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
		expect(readdirSync(r.cache).sort()).toEqual(["image.txt", "libbar_1_amd64.deb", "libfoo_1_amd64.deb", "packages.txt"]);
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
		expect(readdirSync(r.cache).sort()).toEqual(["image.txt", "libbar_1_amd64.deb", "libfoo_1_amd64.deb", "packages.txt"]);
	});

	// Incident (#604): a mirror that crawled held the job for 14 min, then 25+ min. A stalled download is ended and retried once.
	test("a download that stalls is ended at its limit and tried once more, which finishes the job", () => {
		const r = runner();
		const res = r.run({ HANG_DOWNLOADS: "1" });
		expect(res.code).toBe(0);
		expect(r.calls().filter((c) => c.includes("--download-only"))).toHaveLength(2);
		expect(res.out).toContain("the download failed (exit 143, the network)");
		expect(existsSync(join(r.cache, "packages.txt"))).toBe(true);
	});

	test("a mirror that stalls on both rounds fails the step: no third try, no install, nothing cached", () => {
		const r = runner();
		const res = r.run({ HANG_DOWNLOADS: "9" });
		expect(res.code).not.toBe(0);
		expect(r.calls().filter((c) => c.includes("--download-only"))).toHaveLength(2);
		expect(r.calls().filter((c) => c.startsWith("apt-get install") && !c.includes("--download-only"))).toEqual([]);
		expect(res.out).toContain("::error::ci-apt: the apt mirror is too slow or down (the network failed on every round: 2 rounds); rerun the job");
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
		expect(res.out).toContain("apt-get update failed (exit 143, the network) (http://fast.test/ubuntu/)");
		expect(res.out).toContain("installed from the image sources");
	});

	test("an architecture the fast mirror does not serve (arm64 is on ports) never sees it", () => {
		const r = runner();
		expect(r.run({ ...r.fast, CI_APT_ARCH: "aarch64" }).code).toBe(0);
		expect(r.calls().some((c) => c.includes("SourceParts="))).toBe(false);
	});

	// Run 44472459923 (a mirror outage): round 1 could not connect to azure (apt exits 100, not 143), round 2 failed its update;
	// the step failed with no annotation, so the rerun automation and the merge queue took an outage for a code failure.
	test("a connect failure on the fast mirror and an update failure on the image's own sources both end with the annotation", () => {
		const r = runner();
		const res = r.run({ ...r.fast, NET_FAIL: "update" });
		expect(res.code).toBe(1);
		expect(res.out).toContain("apt-get update failed (exit 100, the network) (http://fast.test/ubuntu/)");
		expect(res.out).toMatch(/apt-get update failed \(exit 100, the network\)\n/);
		expect(res.out).toContain("::error::ci-apt: the apt mirror is too slow or down");
		expect(r.calls().filter((c) => c.startsWith("apt-get update"))).toHaveLength(2);
		expect(existsSync(join(r.cache, "packages.txt"))).toBe(false);
	});

	test("files that cannot be fetched (exit 100 with Err: lines) are the network too, in the download as in the update", () => {
		const r = runner();
		const res = r.run({ NET_FAIL: "download", HANG_DOWNLOADS: "0" });
		expect(res.code).toBe(1);
		expect(res.out).toContain("the download failed (exit 100, the network)");
		expect(res.out).toContain("::error::ci-apt: the apt mirror is too slow or down");
	});

	test("an unknown package is a plain failure: no annotation, and no second round for it", () => {
		const r = runner();
		const res = r.run({ ...r.fast, UNKNOWN_PACKAGE: "1" });
		expect(res.code).toBe(100);
		expect(res.out).toContain("the download failed (exit 100, not the network)");
		expect(res.out).not.toContain("::error::");
		expect(r.calls().filter((c) => c.startsWith("apt-get update"))).toHaveLength(1);
	});

	test("an update that fails by itself without a network word is plain too", () => {
		const r = runner();
		const res = r.run({ FAIL_UPDATE: "1" });
		expect(res.code).toBe(100);
		expect(res.out).toContain("apt-get update failed (exit 100, not the network)");
		expect(res.out).not.toContain("::error::");
	});

	// Incident: the key had the image version in it and only main saves, so after each new runner image every pull request
	// went to the mirror until main ran. An older image's cache is now restored and used.
	test("a cache from an older image version is used: its files are seeded, apt fetches only what is newer, the cache is refreshed", () => {
		const r = runner();
		expect(r.run({ CI_APT_IMAGE: "image-1" }).code).toBe(0);
		// A new runner on image 2: apt's archives hold the image's own file only, and the candidate of libfoo is 2 now.
		for (const f of ["libfoo_1_amd64.deb", "libbar_1_amd64.deb"]) rmSync(join(r.archives, f));
		rmSync(join(r.cache, "..", "installed"));
		const warm = r.calls().length;
		const res = r.run({ CI_APT_IMAGE: "image-2", FOO: "2" });
		expect(res.code).toBe(0);
		const after = r.calls().slice(warm);
		expect(after.some((c) => c.startsWith("dpkg -i"))).toBe(false); // not the same-image fast path
		expect(after.map((c) => c.split(" ").slice(0, 2).join(" "))).toEqual(["apt-get update", "apt-get install", "apt-get install"]);
		expect(res.out).toContain("2 cached files from an older runner image put in apt's archives");
		// libbar was reused from the cache; libfoo 1 was replaced by 2 and is not cached any more.
		expect(readdirSync(r.cache).sort()).toEqual(["image.txt", "libbar_1_amd64.deb", "libfoo_2_amd64.deb", "packages.txt"]);
		expect(readFileSync(join(r.cache, "image.txt"), "utf8")).toBe("image-2\n");
		// The next run on image 2 is the fast path.
		const again = r.calls().length;
		expect(r.run({ CI_APT_IMAGE: "image-2" }).code).toBe(0);
		expect(r.calls().slice(again)).toHaveLength(1);
		expect(r.calls().slice(again)[0]).toMatch(/^dpkg -i/);
	});

	test("an older image's cache with the mirror down fails as the network, not as a cache problem", () => {
		const r = runner();
		expect(r.run({ CI_APT_IMAGE: "image-1" }).code).toBe(0);
		const res = r.run({ CI_APT_IMAGE: "image-2", NET_FAIL: "update" });
		expect(res.code).toBe(1);
		expect(res.out).toContain("::error::ci-apt: the apt mirror is too slow or down");
	});

	test("no packages is a usage error, not an install of nothing", () => {
		const r = runner();
		const p = Bun.spawnSync(["bash", script, r.cache], { env: process.env });
		expect(p.exitCode).toBe(2);
	});
});
