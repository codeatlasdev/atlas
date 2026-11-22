import { expect, test } from "bun:test";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// scripts/mobile-apk-release.sh drives `gh` for the mobile tag's GitHub release. The release must never become
// "Latest" (that is what the CLI and desktop installers follow), the tag must get a release as soon as testers have
// the build (2026-11-21: mobile-v1.0.2 had an empty tag page while it waited for the store approval), and the store
// stage must turn that same release into a normal one. A fake `gh` records every call.

const script = join(import.meta.dir, "mobile-apk-release.sh");

function run(stage: string, opts: { exists: boolean; apk: boolean }) {
	const d = mkdtempSync(join(tmpdir(), "apk-rel-"));
	const bin = join(d, "bin");
	mkdirSync(bin);
	const log = join(d, "calls");
	writeFileSync(
		join(bin, "gh"),
		`#!/bin/bash\nprintf '%s\\0' "$@" >> ${JSON.stringify(log)}\nprintf '\\036' >> ${JSON.stringify(log)}\n[ "$1 $2" = "release view" ] && exit ${opts.exists ? 0 : 1}\nexit 0\n`,
	);
	chmodSync(join(bin, "gh"), 0o755);
	const apkDir = join(d, "apk");
	mkdirSync(apkDir);
	if (opts.apk) {
		writeFileSync(join(apkDir, "atlas-mobile-1.0.2.apk"), "apk");
		writeFileSync(join(apkDir, "atlas-mobile-1.0.2.apk.sha256"), "sum");
	}
	const p = Bun.spawnSync(["bash", script, stage, "mobile-v1.0.2", "1.0.2", "29921287", apkDir], {
		env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, GITHUB_REPOSITORY: "codeatlasdev/atlas" },
	});
	let text = "";
	try {
		text = readFileSync(log, "utf8");
	} catch {}
	const calls = text
		.split("\x1e")
		.filter((l) => l.length > 0)
		.map((l) => l.split("\0").filter((a) => a.length > 0))
		.filter((a) => !(a[0] === "release" && a[1] === "view"));
	return { code: p.exitCode, stderr: p.stderr.toString(), calls };
}

test("testing: a new tag gets a pre-release with the APK, never Latest", () => {
	const r = run("testing", { exists: false, apk: true });
	expect(r.code).toBe(0);
	expect(r.calls).toHaveLength(1);
	const c = r.calls[0];
	expect(c.slice(0, 3)).toEqual(["release", "create", "mobile-v1.0.2"]);
	expect(c).toContain("--prerelease");
	expect(c).toContain("--latest=false");
	expect(c).toContain("--verify-tag");
	expect(c.some((a) => a.endsWith("atlas-mobile-1.0.2.apk"))).toBe(true);
	expect(c.some((a) => a.endsWith("atlas-mobile-1.0.2.apk.sha256"))).toBe(true);
	expect(c[c.indexOf("--title") + 1]).toBe("Atlas Mobile 1.0.2 (testing)");
});

test("store: the existing pre-release becomes a normal release, still not Latest", () => {
	const r = run("store", { exists: true, apk: true });
	expect(r.code).toBe(0);
	const [upload, edit] = r.calls;
	expect(upload.slice(0, 3)).toEqual(["release", "upload", "mobile-v1.0.2"]);
	expect(upload).toContain("--clobber");
	expect(edit.slice(0, 3)).toEqual(["release", "edit", "mobile-v1.0.2"]);
	expect(edit).toContain("--prerelease=false");
	expect(edit).toContain("--latest=false");
	expect(edit[edit.indexOf("--title") + 1]).toBe("Atlas Mobile 1.0.2");
	expect(edit[edit.indexOf("--notes") + 1]).toContain("approved for Google Play and the App Store");
});

test("store with no release yet (the testing stage failed): created with the APK, not a pre-release", () => {
	const r = run("store", { exists: false, apk: true });
	expect(r.code).toBe(0);
	const c = r.calls[0];
	expect(c[1]).toBe("create");
	expect(c).toContain("--prerelease=false");
	expect(c).toContain("--latest=false");
});

test("every call that sets flags says --latest=false", () => {
	for (const stage of ["testing", "store"])
		for (const exists of [true, false]) {
			const r = run(stage, { exists, apk: true });
			for (const c of r.calls.filter((c) => c[1] === "create" || c[1] === "edit")) expect(c).toContain("--latest=false");
		}
});

test("no APK and no release: fails with what to run, creates nothing", () => {
	const r = run("testing", { exists: false, apk: false });
	expect(r.code).not.toBe(0);
	expect(r.stderr).toContain("fastlane android_apk");
	expect(r.calls).toHaveLength(0);
});

test("an unknown stage is refused", () => {
	const r = run("prod", { exists: true, apk: true });
	expect(r.code).not.toBe(0);
	expect(r.calls).toHaveLength(0);
});
