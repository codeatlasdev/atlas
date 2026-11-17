// ci-paths.sh decides which CI jobs a change skips. A source change classified as docs ships untested code;
// so the rule is: only known docs-only patterns may skip anything, and an unknown path runs everything.
import { PLATFORM_SNIFFING, RATCHETS, SHORTCUTS } from "./i18n/ratchets.ts";
import { shArgv } from "./sh";
import { expect, test } from "bun:test";
import { join } from "node:path";

const script = join(import.meta.dir, "ci-paths.sh");
function areas(files: string[]): Record<string, boolean> {
	const p = Bun.spawnSync(shArgv(script), { stdin: new Blob([files.join("\n")]) });
	return Object.fromEntries(
		p.stdout.toString().trim().split("\n").map((l) => {
			const [k, v] = l.split("=");
			return [k, v === "true"];
		}),
	);
}
const none = { rust: false, front: false, mobile: false, site: false, relay: false, workflows: false, installer: false, i18n: false };

test("AGENTS.md and the other top-level markdown run nothing heavy", () => {
	expect(areas(["AGENTS.md", "CONTRIBUTING.md", "SECURITY.md"])).toEqual(none);
});

test("README and CHANGELOG feed only the i18n job (the writing lint reads them)", () => {
	expect(areas(["README.md", "CHANGELOG.md"])).toEqual({ ...none, i18n: true });
});

test("docs/ rebuilds the site, which renders it, and reruns the i18n job (source hashes, writing lint)", () => {
	expect(areas(["docs/contributing.md"])).toEqual({ ...none, site: true, i18n: true });
});

test("a crate manifest runs Rust only; a crate's sources also run the i18n job (t! references, the ratchet); the desktop frontend also rebuilds the shell that embeds it", () => {
	expect(areas(["crates/atlas-core/Cargo.toml"])).toEqual({ ...none, rust: true });
	expect(areas(["crates/atlas-host/Cargo.toml"])).toEqual({ ...none, rust: true });
	expect(areas(["crates/atlas-host/src/lib.rs"])).toEqual({ ...none, rust: true, i18n: true });
	expect(areas(["apps/desktop/src/App.svelte"])).toEqual({ ...none, rust: true, front: true, i18n: true });
	expect(areas(["Cargo.lock"])).toEqual({ ...none, rust: true });
});

test("markdown that is source is not skipped: skills are embedded and checked", () => {
	expect(areas(["skills/atlas/yaml.md"])).toMatchObject({ rust: true, front: true, i18n: true });
});

test("the Windows installer runs its own job, and only the changes that can affect it do", () => {
	expect(areas(["site/public/install.ps1"])).toEqual({ ...none, site: true, installer: true });
	expect(areas(["scripts/install-ps1.test.ts"])).toEqual({ ...none, front: true, installer: true });
	expect(areas(["site/public/install.sh"])).toEqual({ ...none, site: true });
});

test("unknown paths, workflows and an empty diff run everything", () => {
	const all = { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true, i18n: true };
	expect(areas(["package.json"])).toEqual(all);
	expect(areas(["scripts/release.sh"])).toEqual(all);
	expect(areas([])).toEqual(all);
	expect(areas([".github/workflows/ci.yml"])).toMatchObject({ rust: true, workflows: true });
});

// bun run i18n:check runs in its own job: every file it reads must reach it, or a red catalog, a new println! or an em
// dash in the docs merges. Each case below is a path whose misclassification lets one of those through.
test.each([
	["docs/pt-BR/getting-started.md", { site: true }],
	["site/project.inlang/settings.json", { site: true }],
	["site/src/components/islands/Waitlist.svelte", { site: true }],
	["site/src/pages/index.astro", { site: true }],
	["site/src/content/guide.md", {}],
	["crates/atlas-core/src/lang.rs", { rust: true }],
	["crates/atlas-i18n/src/lib.rs", { rust: true }],
	["crates/atlas-cli/tests/cli_i18n.rs", { rust: true }],
	["crates/atlas-cli/src/commands/config.rs", { rust: true }],
	["crates/atlas-tui/src/tui/mod.rs", { rust: true }],
	["crates/atlas-desktop/src/menu.rs", { rust: true }],
	["crates/atlas-host/src/server.rs", { rust: true }],
	["apps/desktop/src/lib/i18n/fmt.ts", { rust: true, front: true }],
	["i18n/messages/app/en.json", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["i18n/messages/common/lock.json", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["i18n/surfaces.json", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["apps/desktop/project.inlang/settings.json", { rust: true, front: true }],
	["i18n/glossary.yaml", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["i18n/hardcoded-baseline.json", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["i18n/README.md", {}],
	["i18n/style/flagged-terms.yaml", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["scripts/i18n/check.ts", { rust: true, front: true, mobile: true, site: true, relay: true, workflows: true, installer: true }],
	["skills/atlas/SKILL.md", { rust: true, front: true }],
	["README.md", {}],
	["CHANGELOG.md", {}],
])("%s reaches the i18n job", (file, others) => {
	expect(areas([file])).toEqual({ ...none, ...others, i18n: true });
});

test.each([
	["a crate manifest", "crates/atlas-core/Cargo.toml"],
	["the Flutter app", "apps/mobile/lib/main.dart"],
	["the push relay", "services/push-relay/main.go"],
	["a site asset", "site/public/llms.txt"],
	["a top-level doc", "AGENTS.md"],
])("%s does not rerun the i18n job", (_n, file) => {
	expect(areas([file]).i18n).toBe(false);
});

// The list above is hand-written; the ratchets are code. When a ratchet learns a new directory this fails until
// ci-paths.sh sends it to the i18n job.
test("every directory a ratchet scans is classified into the i18n job", () => {
	const dirs = RATCHETS.flatMap((r) => r.scopes.flatMap((s) => s.dirs));
	expect(dirs.length).toBeGreaterThan(5);
	for (const dir of dirs) {
		const file = /\.[a-z]+$/.test(dir) ? dir : `${dir}/probe.rs`;
		expect(areas([file]).i18n, file).toBe(true);
	}
	for (const def of [SHORTCUTS, PLATFORM_SNIFFING]) {
		for (const s of def.scopes) expect(areas([`${s.dirs[0]}/x.ts`]).i18n, s.dirs[0]).toBe(true);
	}
});
