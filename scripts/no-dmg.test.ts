// Incident: Finder kept opening on the "Atlas" installer image on the maintainer's screen. Every local/agent debug
// build ran a full `cargo tauri build --debug`, whose bundle step includes the DMG target; Tauri's bundle_dmg.sh then
// mounts the image and drives Finder (AppleScript) to lay the window out. Local, debug and agent builds make the .app
// only; the DMG is built by the release job alone, and even there with `--ci` (no Finder scripting).
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (p: string) => readFileSync(join(root, p), "utf8");
const pkg: { scripts: Record<string, string> } = JSON.parse(read("package.json"));

/** The `cargo tauri build …` commands in a text (a script line or a hint inside a string). */
const tauriBuilds = (text: string) => [...text.matchAll(/cargo tauri build[^\n"'`)]*/g)].map((m) => m[0]);

describe("local, debug and agent builds never build the DMG", () => {
	test("`desktop:build:app` is the one app-only entry: debug, app bundle only, no Finder scripting", () => {
		const cmd = pkg.scripts["desktop:build:app"];
		expect(cmd).toBeDefined();
		expect(cmd).toMatch(/cargo tauri build\b/);
		expect(cmd).toMatch(/--debug\b/);
		expect(cmd).toMatch(/--bundles app(\s|$)/);
		expect(cmd).not.toMatch(/dmg/);
	});

	test("desktop-agent.sh builds through that entry, never the full bundle, and does not unset CI", () => {
		const sh = read("scripts/desktop-agent.sh");
		expect(sh).toMatch(/desktop:build:app/);
		expect(sh).not.toMatch(/desktop:build(?!:app)/);
		// `CI` set is what makes tauri's bundle_dmg.sh skip Finder; unsetting it re-enabled the AppleScript layout
		expect(sh).not.toMatch(/env -u CI/);
	});

	test("every hint that tells a human or an agent how to build the debug bundle names the app-only command", () => {
		for (const f of ["studio/src/capture/app.ts", "crates/atlas-cli/src/commands/desktop_agent.rs"]) {
			const t = read(f);
			expect(t, f).not.toMatch(/desktop:build -- --debug/);
			expect(t, f).toMatch(/desktop:build:app/);
		}
	});

	test("`atlas link --desktop` already builds `--bundles app`", () => {
		expect(read("crates/atlas-cli/src/commands/link.rs")).toMatch(/"--debug",\s*"--bundles",\s*"app"/);
	});

	test("no script or hint runs a debug `cargo tauri build` that includes the dmg", () => {
		const files = ["package.json", "scripts/desktop-agent.sh", "scripts/release.sh", "scripts/setup.sh", "studio/src/capture/app.ts"];
		for (const f of files)
			for (const c of tauriBuilds(read(f)))
				if (/--debug/.test(c)) expect(c, `${f}: ${c}`).toMatch(/--bundles app(\s|$)/);
	});
});

describe("the release DMG does not script Finder", () => {
	test("the release job builds app+dmg with --ci, which makes bundle_dmg.sh skip the Finder layout", () => {
		const wf = read(".github/workflows/release.yml");
		const builds = tauriBuilds(wf).filter((c) => /dmg/.test(c));
		expect(builds.length).toBeGreaterThan(0);
		for (const c of builds) expect(c).toMatch(/--ci\b/);
	});

	test("the plain `desktop:build` (release bundle on the maintainer's machine) passes --ci too", () => {
		expect(pkg.scripts["desktop:build"]).toMatch(/cargo tauri build\b.*--ci\b/);
	});
});
