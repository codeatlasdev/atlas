// scripts/linux-pkg-deps.sh: what stops a release package that builds and installs but leaves out a runtime library or
// tool (#267: no ripgrep for project search; #265: no GStreamer plugins, no tray library). The tools the script reads
// (dpkg-deb, rpm, pacman) are shimmed with output in each tool's own shape; the verdicts are the script's.
import { shArgv } from "./sh";
import { afterEach, describe, expect, test } from "bun:test";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const dirs: string[] = [];
afterEach(() => {
	for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

/** Run the script on `file`, with `tool` (dpkg-deb | rpm | pacman) printing `output`. */
function check(file: string, tool: string, output: string) {
	const d = mkdtempSync(join(tmpdir(), "pkg-deps-"));
	dirs.push(d);
	mkdirSync(join(d, "bin"));
	writeFileSync(join(d, "out.txt"), output);
	writeFileSync(join(d, "bin", tool), `#!/bin/sh\ncat '${join(d, "out.txt")}'\n`);
	chmodSync(join(d, "bin", tool), 0o755);
	writeFileSync(join(d, file), "");
	const p = Bun.spawnSync(shArgv(join(root, "scripts/linux-pkg-deps.sh"), join(d, file)), {
		env: { ...process.env, PATH: `${join(d, "bin")}:${process.env.PATH}` },
	});
	return { code: p.exitCode, out: p.stdout.toString() };
}

const DEB = "libwebkit2gtk-4.1-0 (>= 2.40), libgtk-3-0 (>= 3.24), libayatana-appindicator3-1, git, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good, ripgrep\n";
// `rpm -qpR` of the release .rpm (verify-linux, Fedora): sonames for the libraries, names for what Atlas adds.
const RPM = ["git", "ripgrep", "gstreamer1-plugins-base", "gstreamer1-plugins-good", "libayatana-appindicator3.so.1()(64bit)", "libwebkit2gtk-4.1.so.0()(64bit)", "libgtk-3.so.0()(64bit)", "rpmlib(CompressedFileNames) <= 3.0.4-1", "/bin/sh"].join("\n") + "\n";
const ARCH = (deps: string) =>
	`Name            : codeatlas-atlas-bin\nVersion         : 1.5.0alpha.26-1\nDepends On      : ${deps}\nOptional Deps   : docker: infra: services in atlas.yaml\n                  github-cli: pull requests and checks in Atlas Desktop\n`;
const ARCH_ALL = "webkit2gtk-4.1  gst-plugins-base  gst-plugins-good  gtk3  git  libayatana-appindicator  ripgrep";
const ARCH_FILE = "codeatlas-atlas-bin-1.0.0-1-x86_64.pkg.tar.zst";

describe.skipIf(process.platform === "win32")("linux-pkg-deps.sh", () => {
	test("a .deb, an .rpm and an Arch package that declare everything pass", () => {
		for (const [file, tool, out] of [
			["atlas_1.0.0_amd64.deb", "dpkg-deb", DEB],
			["atlas-1.0.0-1.x86_64.rpm", "rpm", RPM],
			[ARCH_FILE, "pacman", ARCH(ARCH_ALL)],
		] as const) {
			const r = check(file, tool, out);
			expect({ file, code: r.code, out: r.out }).toMatchObject({ code: 0 });
			expect(r.out).not.toContain("MISSING");
		}
	});

	// #267: only `git` was declared, yet project search runs `rg`.
	test("a package without ripgrep fails and says so", () => {
		const r = check("atlas_1.0.0_amd64.deb", "dpkg-deb", DEB.replace(", ripgrep", ""));
		expect(r.code).toBe(1);
		expect(r.out).toContain("MISSING  ripgrep");
		expect(r.out).not.toContain("MISSING  git");
	});

	// #265: webkit2gtk pulls in gst-plugins-base-libs; the demuxers and decoders are in gst-plugins-base.
	test("`gst-plugins-base-libs` does not stand in for `gst-plugins-base`", () => {
		const r = check(ARCH_FILE, "pacman", ARCH(ARCH_ALL.replace("gst-plugins-base ", "gst-plugins-base-libs ")));
		expect(r.code).toBe(1);
		expect(r.out).toContain("MISSING  GStreamer base plugins");
	});

	test("an optional dependency is not a dependency", () => {
		const optional = ARCH(ARCH_ALL.replace("  ripgrep", "")).replace("docker:", "ripgrep: search, docker:");
		const r = check(ARCH_FILE, "pacman", optional);
		expect(r.code).toBe(1);
		expect(r.out).toContain("MISSING  ripgrep");
	});

	test("rpm versions and rpmlib lines are not package names, and a missing tray library fails", () => {
		const rpm = RPM.replace("libayatana-appindicator3.so.1()(64bit)\n", "").replace("libwebkit2gtk-4.1.so.0()(64bit)", "libwebkit2gtk-4.1.so.0()(64bit) >= 2.40");
		const r = check("atlas-1.0.0-1.x86_64.rpm", "rpm", rpm);
		expect(r.code).toBe(1);
		expect(r.out).toContain("MISSING  tray library");
		expect(r.out).not.toContain("MISSING  WebKitGTK");
	});

	test("a file that is not a package is a usage error (2), not a pass", () => {
		expect(check("atlas.tar.gz", "pacman", "").code).toBe(2);
		expect(Bun.spawnSync(shArgv(join(root, "scripts/linux-pkg-deps.sh"), "/nonexistent.deb")).exitCode).toBe(2);
	});
});
