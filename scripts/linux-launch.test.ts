// The Linux launch gate (#283): what stops a release that only builds. These tests pin the wiring a release depends on
// (publish waits for verify-linux, a build-only run can never publish, every build checks out the commit it names) and
// the launch check's verdicts on a real private sway when the machine has one.
import { shArgv } from "./sh";
import { describe, expect, test } from "bun:test";
import { chmodSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");
const release = Bun.YAML.parse(read(".github/workflows/release.yml")) as any;
const ci = Bun.YAML.parse(read(".github/workflows/ci.yml")) as any;

describe("release.yml", () => {
	test("publish waits for verify-linux and its `if` requires it to have succeeded", () => {
		expect(release.jobs.publish.needs).toContain("verify-linux");
		expect(release.jobs.publish.if).toContain("needs.verify-linux.result == 'success'");
	});

	test("a build-only run can never publish or sign", () => {
		expect(release.jobs.publish.if).toContain("!inputs.build_only");
		for (const j of ["sign-binaries", "sign-installer", "sign-setup"]) expect(release.jobs[j].if).toContain("!inputs.build_only");
		expect(release.on.workflow_dispatch.inputs.build_only.default).toBe(false);
	});

	test("only publish may write to the repository", () => {
		for (const [name, job] of Object.entries<any>(release.jobs))
			if (name !== "publish") expect(job.permissions?.contents ?? "read", name).not.toBe("write");
	});

	test("every checkout of a build job uses REF (the tag, or the dispatched commit in a build-only run), never the tag directly", () => {
		expect(read(".github/workflows/release.yml")).not.toMatch(/ref: refs\/tags\/\$\{\{ env\.TAG \}\}/);
		for (const [name, job] of Object.entries<any>(release.jobs))
			for (const s of job.steps ?? [])
				if (String(s.uses ?? "").startsWith("actions/checkout") && s.with?.ref) expect(s.with.ref, name).toBe("${{ env.REF }}");
	});

	test("verify-linux opens the AppImage and the packaged app in the four distros", () => {
		const job = release.jobs["verify-linux"];
		expect(job.strategy.matrix.include.map((m: any) => m.image)).toEqual(["archlinux:latest", "fedora:latest", "ubuntu:24.04", "debian:stable"]);
		const runs = job.steps.map((s: any) => s.run ?? "").join("\n");
		expect(runs).toContain("atlas doctor --json");
		expect(runs.match(/scripts\/linux-launch-check\.sh/g)?.length).toBe(2);
		expect(runs).toContain(".AppImage");
	});
});

// #265: a package install without GStreamer's plugins aborts the web process as soon as a page loads an <audio> or
// <video> (MediaViewer), and no Web Audio decode works. The runtime packages webkit2gtk pulls in are only the libraries.
describe("the .deb and .rpm", () => {
	const conf = JSON.parse(read("crates/atlas-desktop/tauri.linux.conf.json"));
	test("the .deb depends on the Debian/Ubuntu GStreamer base and good plugin packages", () => {
		expect(conf.bundle.linux.deb.depends).toEqual(expect.arrayContaining(["gstreamer1.0-plugins-base", "gstreamer1.0-plugins-good"]));
	});
	test("the .rpm depends on the Fedora GStreamer base and good plugin packages", () => {
		expect(conf.bundle.linux.rpm.depends).toEqual(expect.arrayContaining(["gstreamer1-plugins-base", "gstreamer1-plugins-good"]));
	});
	test("both keep git, which atlas needs", () => {
		expect(conf.bundle.linux.deb.depends).toContain("git");
		expect(conf.bundle.linux.rpm.depends).toContain("git");
	});
	test("verify-linux checks, after the install, that the plugins are on disk", () => {
		const steps = release.jobs["verify-linux"].steps.map((s: any) => s.run ?? "").join("\n");
		expect(steps).toContain("libgstogg.so");
	});
});

describe("the Arch package", () => {
	// Incident: verify-linux (build-only run 43766908511): `pacman -U` of the package, then atlas-desktop panicked at start
	// ("Failed to load ayatana-appindicator3") because libayatana-appindicator was only an optdepend. The tray library is
	// dlopen'd by the app, so a hard dependency of every Arch install.
	test("depends on the tray library atlas-desktop loads at startup, not just optdepends it", () => {
		const pkgbuild = read("packaging/arch/PKGBUILD.in");
		const depends = pkgbuild.match(/^depends=\((.*)\)$/m)?.[1] ?? "";
		expect(depends).toContain("'libayatana-appindicator'");
		expect(pkgbuild).not.toMatch(/optdepends=\([^)]*libayatana-appindicator/);
	});

	// #265, measured on an Arch install with webkit2gtk-4.1 and only gst-plugins-base-libs (what webkit2gtk-4.1 pulls in):
	// every Web Audio decode failed with `Missing decoder: Ogg`/`WAV`, `GStreamer element autoaudiosink not found`, and a
	// page that loaded an <audio>/<video> aborted its web process (SIGABRT). The demuxers and decoders are in the plugin
	// packages, not the -libs ones.
	test("depends on the GStreamer plugins webkit2gtk-4.1 needs to play media, not just its -libs", () => {
		const depends = read("packaging/arch/PKGBUILD.in").match(/^depends=\((.*)\)$/m)?.[1] ?? "";
		expect(depends).toContain("'gst-plugins-base'");
		expect(depends).toContain("'gst-plugins-good'");
	});
});

describe("ci.yml", () => {
	test("the Linux launch job is part of the final CI gate", () => {
		expect(ci.jobs["desktop-linux"]).toBeDefined();
		expect(ci.jobs.ci.needs).toContain("desktop-linux");
	});
});

describe("linux-launch-check.sh", () => {
	const run = (...args: string[]) => Bun.spawnSync(shArgv(join(root, "scripts/linux-launch-check.sh"), ...args), { cwd: root });
	const hasSway = ["sway", "grim", "dbus-run-session"].every((t) => Bun.which(t));

	test("a binary that does not exist is a usage error (2), not a pass", () => {
		expect(run("/nonexistent/atlas-desktop").exitCode).toBe(2);
	});

	// Incident class: a launch check that reports success for an app that never opened.
	test.skipIf(!hasSway)("an app that exits at once fails with app-exited (1)", () => {
		const out = mkdtempSync(join(tmpdir(), "launch-test-"));
		const p = run("--out", out, "--map-timeout", "20", "/bin/false");
		expect(p.exitCode).toBe(1);
		expect(p.stdout.toString()).toContain("FAIL app-exited");
		expect(readFileSync(join(out, "result.txt"), "utf8")).toContain("result: fail");
	});

	test.skipIf(!hasSway)("a program that runs but never opens a window fails with no-window (1)", () => {
		const out = mkdtempSync(join(tmpdir(), "launch-test-"));
		const app = join(out, "silent-app");
		writeFileSync(app, "#!/bin/sh\nexec sleep 30\n");
		chmodSync(app, 0o755);
		const p = run("--out", out, "--map-timeout", "3", app);
		expect(p.exitCode).toBe(1);
		expect(p.stdout.toString()).toContain("FAIL no-window");
	});

	// #265: the lines are copied from the real stderr of the published 1.5.0-alpha.25 AppImage (no GStreamer plugins) when
	// its page created an AudioContext, and of the AppImage built with the plugins. Only the first is a broken package;
	// a machine without an audio sink or an AAC decoder is the host's gap, not a failed launch.
	describe("stderr markers", () => {
		const markers = new RegExp(read("scripts/linux-launch-check.sh").match(/^MARKERS='(.*)'$/m)![1]!);
		test("`appsrc not found` (bundled GStreamer without its plugins) fails the launch", () => {
			expect(markers.test("GStreamer element appsrc not found. Please install it")).toBe(true);
		});
		test("a host with no audio sink or no AAC decoder is not a launch failure", () => {
			expect(markers.test("GStreamer element autoaudiosink not found. Please install it")).toBe(false);
			expect(markers.test("Missing decoder: MPEG-4 AAC (audio/x-m4a)")).toBe(false);
		});
	});
});
