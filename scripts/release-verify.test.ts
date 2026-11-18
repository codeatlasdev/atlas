// #312: every shipped artifact is executed before publish. Linux has verify-linux (linux-launch.test.ts); these pin
// the same for Windows (the NSIS installer and Atlas-Setup.exe, silent install, open, uninstall) and macOS (.dmg,
// .app.tar.gz). The scripts themselves run on a native runner; what can be proven here is the wiring around them,
// that the check fails when it must, and that it names the files the release really builds.
import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");
const release = Bun.YAML.parse(read(".github/workflows/release.yml")) as any;
const runs = (job: string) => release.jobs[job].steps.map((s: any) => s.run ?? "").join("\n");
const uploads = (job: string): string[] => release.jobs[job].steps.filter((s: any) => String(s.uses ?? "").startsWith("actions/upload-artifact")).map((s: any) => s.with.name);
const downloads = (job: string): string[] => release.jobs[job].steps.filter((s: any) => String(s.uses ?? "").startsWith("actions/download-artifact")).map((s: any) => s.with.name);

describe("release.yml verifies the Windows and macOS artifacts before publish", () => {
	test.each([
		["verify-windows", "build-windows", "atlas-x86_64-pc-windows-msvc"],
		["verify-macos", "build", "atlas-aarch64-apple-darwin"],
	])("%s runs on a native runner against exactly what %s uploads, and publish needs it to have succeeded", (job, builder, artifact) => {
		expect(release.jobs[job].needs).toBe(builder);
		expect(uploads(builder)).toContain(artifact);
		expect(downloads(job)).toEqual([artifact]);
		expect(release.jobs.publish.needs).toContain(job);
		expect(release.jobs.publish.if).toContain(`needs.${job}.result == 'success'`);
	});

	test("verify-windows is not skipped with the optional SignPath jobs behind build-windows", () => {
		// A job without an `if` is skipped when any job upstream was skipped, and an unsigned build skips the signing jobs.
		expect(release.jobs["verify-windows"].if).toContain("!cancelled()");
		expect(release.jobs["verify-windows"].if).toContain("needs.build-windows.result == 'success'");
	});

	test("the jobs run on the platform they verify, and open the app before anything is published", () => {
		expect(release.jobs["verify-windows"]["runs-on"]).toMatch(/^windows-/);
		expect(release.jobs["verify-macos"]["runs-on"]).toMatch(/^macos-/);
		expect(runs("verify-windows")).toContain("scripts/windows-launch-check.ps1");
		expect(runs("verify-macos")).toContain("scripts/macos-launch-check.sh");
	});

	test("each job proves its check can fail before trusting its pass", () => {
		const w = release.jobs["verify-windows"].steps.map((s: any) => s.run ?? "");
		const m = release.jobs["verify-macos"].steps.map((s: any) => s.run ?? "");
		expect(w.findIndex((r: string) => r.includes("-Negative"))).toBeGreaterThan(-1);
		expect(w.findIndex((r: string) => r.includes("-Negative"))).toBeLessThan(w.findIndex((r: string) => r.includes("-Version")));
		expect(m.findIndex((r: string) => r.includes("--negative"))).toBeGreaterThan(-1);
		expect(m.findIndex((r: string) => r.includes("--negative"))).toBeLessThan(m.findIndex((r: string) => r.includes("--version") || r.includes("VERSION_ARGS")));
	});

	test("a build-only run does not claim the throwaway version is inside the binaries", () => {
		for (const job of ["verify-windows", "verify-macos"]) expect(release.jobs[job].env.BUILD_ONLY).toBe("${{ inputs.build_only }}");
		expect(runs("verify-windows")).toContain("$env:BUILD_ONLY -eq 'true'");
		expect(runs("verify-macos")).toContain('"$BUILD_ONLY" != true');
	});

	test("the Windows script executes the files the release builds and the manifest lists", () => {
		const ps = read("scripts/windows-launch-check.ps1");
		const manifest = read("scripts/generate-manifest.py");
		// The NSIS installer is `Atlas-<v>-<target>-setup.exe` (the manifest's `desktop` asset), the one-click one is Atlas-Setup.exe.
		expect(manifest).toContain("-setup.exe");
		expect(ps).toContain('"Atlas-*-$target-setup.exe"');
		expect(ps).toContain("Atlas-Setup.exe");
		expect(read(".github/workflows/release.yml")).toContain('cp in/setup/Atlas-Setup.exe dist/Atlas-Setup.exe');
		// The install dir and the uninstaller names are the ones crates/atlas-setup and tauri's NSIS use.
		expect(ps).toContain("Atlas Uninstall.exe");
		expect(read("crates/atlas-setup/src/cli.rs")).toContain('"--silent" | "/s"');
	});

	test("the macOS script executes the files the release builds and the manifest lists", () => {
		const sh = read("scripts/macos-launch-check.sh");
		const manifest = read("scripts/generate-manifest.py");
		expect(manifest).toContain(".app.tar.gz");
		expect(manifest).toContain(".dmg");
		for (const part of ['-"$target".tar.gz', '-"$target".dmg', '-"$target".app.tar.gz', "codesign --verify --deep --strict"]) expect(sh).toContain(part);
	});
});

// The script that can run anywhere bash does: its verdicts on a dist directory that is not a release.
describe.skipIf(process.platform === "win32")("macos-launch-check.sh fails a release that is not whole", () => {
	const script = join(root, "scripts/macos-launch-check.sh");
	const run = (args: string[]) => Bun.spawnSync(["bash", script, ...args], { stdout: "pipe", stderr: "pipe" });
	const text = (r: { stdout: Uint8Array; stderr: Uint8Array }) => `${new TextDecoder().decode(r.stdout)}${new TextDecoder().decode(r.stderr)}`;

	test("the negative control: a program that is not an app fails the launch check, and that is a pass", () => {
		const out = mkdtempSync(join(tmpdir(), "mac-neg-"));
		const r = run(["--negative", "--out", out]);
		expect(r.exitCode, text(r)).toBe(0);
		expect(text(r)).toContain("the check failed as it must");
	});

	test("a dist without the release files is an error, never a pass", () => {
		const dist = mkdtempSync(join(tmpdir(), "mac-dist-"));
		const r = run(["--out", mkdtempSync(join(tmpdir(), "mac-out-")), dist]);
		expect(r.exitCode).toBe(1);
		expect(text(r)).toContain("a release file is missing");
	});

	test("a file that does not match its checksum fails the check", () => {
		const dist = mkdtempSync(join(tmpdir(), "mac-dist-"));
		const out = mkdtempSync(join(tmpdir(), "mac-out-"));
		const t = "aarch64-apple-darwin";
		for (const name of [`atlas-1.0.0-${t}.tar.gz`, `Atlas-1.0.0-${t}.dmg`, `Atlas-1.0.0-${t}.app.tar.gz`]) {
			writeFileSync(join(dist, name), "bytes");
			writeFileSync(join(dist, `${name}.sha256`), `${"0".repeat(64)}  ${name}\n`);
		}
		const r = run(["--out", out, dist]);
		expect(r.exitCode, text(r)).toBe(1);
		expect(readFileSync(join(out, "result.txt"), "utf8")).toContain("FAIL checksum");
	});
});
