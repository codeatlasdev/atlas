// The one map from changed paths to checks. `bun run check` (every area), `bun run check:changed` (the areas a
// change touches) and CI's plan job (scripts/ci-plan.ts: one `area=true|false` output per area, which gates the jobs
// of .github/workflows/ci.yml) all read this file and nothing else. To add a check: add it to CHECKS under its area.
// To teach the planner a path: add a glob to the area it affects. A path no area claims runs EVERYTHING (an unknown
// path never skips a check); scripts/checks/map.test.ts fails when CI and this file disagree.

/** Area -> globs of the files that can affect it (matched against forward-slash paths relative to the repo root). */
export const AREAS: Record<string, string[]> = {
	// The desktop shell embeds the frontend build and the CLI embeds the skills, so those rebuild crates too.
	rust: [
		"crates/**",
		"Cargo.toml",
		"Cargo.lock",
		"rust-toolchain.toml",
		"deny.toml",
		".cargo/**",
		".config/nextest.toml",
		"apps/desktop/**",
		"skills/**",
		"scripts/linux-launch-check.sh",
	],
	// The built-in agent list (crates/atlas-core/data/agents.json) is imported by the desktop frontend and generated into
	// Atlas Mobile's Dart; the scripts area runs the staleness test of that copy.
	desktop: ["apps/desktop/**", "crates/atlas-core/data/agents.json"],
	skills: ["skills/**", "scripts/check-skills.sh"],
	// The repo's own scripts, release helpers and everything their tests read.
	scripts: [
		"scripts/**",
		".claude/**", // agent guard wiring, read by scripts/agent-guard*.test.ts
		".codex/**",
		".omp/**",
		"CHANGELOG.md",
		"testdata/**",
		"packaging/**",
		".github/workflows/**",
		".github/actions/**",
		".github/labels.yml",
		".github/release.yml",
		".github/release-contributors.json",
		"site/public/install.{sh,ps1}",
		"crates/atlas-desktop/tauri*.json",
		"crates/atlas-desktop/src/main.rs",
		"crates/atlas-cli/src/commands/{link,desktop_agent}.rs",
		"apps/desktop/src/lib/debug/selectionBench.ts",
		"crates/atlas-core/data/agents.json",
	],
	mobile: ["apps/mobile/**", "scripts/mobile-check.sh", "crates/atlas-core/data/agents.json"],
	relay: ["services/push-relay/**", "scripts/push-relay-check.sh"],
	// docs/ is rendered by the site.
	site: ["site/**", "docs/**"],
	// The Windows installer has its own job (PowerShell 5.1 and 7).
	installer: ["site/public/install.ps1", "scripts/install-ps1.test.ts"],
	workflows: [".github/workflows/**", ".github/actions/**", ".github/zizmor.yml", ".github/dependabot.yml", "scripts/lint-workflows.sh"],
	// jscpd's own config; the source files it reads are in OVERLAY.
	dup: [".jscpd.json"],
};

/** Globs that turn an area on without making a path known: a source file under an unknown directory still runs
 * everything, not just the duplication check. */
export const OVERLAY: Record<string, string[]> = {
	dup: ["**/*.{rs,ts,js,svelte,astro,dart}"],
};

/** Paths that affect no check (docs and metadata). Only consulted for a path no area claims. */
export const IGNORE: string[] = [
	"*.md",
	"LICENSE*",
	"NOTICE",
	"CITATION.cff",
	".editorconfig",
	".github/CODEOWNERS",
	".github/ISSUE_TEMPLATE/**",
	"assets/**",
];

/** In CI only: a change to these reruns every job (the job definitions themselves changed). Locally they are just
 * their own area (workflows): linting them is what an edit loop needs. */
export const CI_RUNS_ALL: string[] = [".github/workflows/**", ".github/actions/**"];

/** Rust files that are not under `crates/<name>/` but rebuild one crate: glob -> crate. */
export const EMBEDS: [string, string][] = [
	["apps/desktop/**", "atlas-desktop"],
	["skills/**", "atlas-skills"],
];

export interface Check {
	/** Short name, unique. */
	id: string;
	/** The area whose files make it run. */
	area: string;
	/** The command, run by `bash -c` at the repo root. `{cargo}` is `--workspace`, or `-p a -p b` for the crates a
	 * change touches and their dependents. No `cmd`: CI runs it, a dev machine cannot (`why` says why). */
	cmd?: string;
	/** check:changed only: run it just when one of these globs matches a changed file (`check` always runs it). */
	only?: string[];
	/** The ci.yml jobs that run it. */
	ci: string[];
	/** What CI's job text contains when that differs from `cmd` (a different working directory, an action input). */
	ciRun?: string;
	why?: string;
}

/** Every check, in the order `check` runs them. */
export const CHECKS: Check[] = [
	{ id: "fmt", area: "rust", cmd: "cargo fmt --all -- --check", ci: ["rust-macos", "rust-linux", "rust-windows"] },
	{
		id: "clippy",
		area: "rust",
		cmd: "cargo clippy {cargo} --all-targets --locked -- -D warnings",
		ci: ["rust-macos", "rust-linux", "rust-windows"],
	},
	{
		id: "nextest",
		area: "rust",
		cmd: "cargo nextest run {cargo} --locked",
		ci: ["rust-macos", "rust-linux", "rust-windows"],
	},
	{ id: "doctests", area: "rust", cmd: "cargo test {cargo} --locked --doc", ci: ["rust-macos", "rust-linux", "rust-windows"] },
	{
		id: "deny",
		area: "rust",
		cmd: "cargo deny check",
		only: ["**/Cargo.toml", "Cargo.lock", "deny.toml"],
		ci: ["deny"],
		ciRun: "cargo-deny-action",
	},
	{
		id: "desktop-opens",
		area: "rust",
		ci: ["desktop-linux"],
		ciRun: "scripts/linux-launch-check.sh",
		why: "opens the Linux build on a headless sway (WebKitGTK, Mesa llvmpipe); run scripts/linux-launch-check.sh on a Linux machine with sway",
	},
	{
		id: "root-tests",
		area: "rust",
		ci: ["rust-linux-root"],
		ciRun: 'sudo -E env "PATH=$PATH" cargo test -p atlas-host --lib --locked crew::spawn -- --ignored',
		why: "needs root and useradd on a throwaway machine (docs/contributing.md, Tests that need root)",
	},
	{
		id: "arm64-build",
		area: "rust",
		ci: ["rust-linux-arm64"],
		ciRun: "cargo build --locked -p atlas-cli",
		why: "builds on a native Linux arm64 runner",
	},
	{ id: "skills", area: "skills", cmd: "bun scripts/sh.ts scripts/check-skills.sh", ci: ["frontend"], ciRun: "scripts/check-skills.sh" },
	{ id: "workflows", area: "workflows", cmd: "bun scripts/sh.ts scripts/lint-workflows.sh", ci: ["workflows"], ciRun: "scripts/lint-workflows.sh" },
	{ id: "desktop", area: "desktop", cmd: "bun run desktop:check", ci: ["frontend"] },
	{ id: "mobile", area: "mobile", cmd: "bun scripts/sh.ts scripts/mobile-check.sh", ci: ["mobile"], ciRun: "scripts/mobile-check.sh" },
	{ id: "relay", area: "relay", cmd: "bun scripts/sh.ts scripts/push-relay-check.sh", ci: ["push-relay"], ciRun: "scripts/push-relay-check.sh" },
	{ id: "scripts", area: "scripts", cmd: "bun run release:test", ci: ["scripts"] },
	{ id: "dup", area: "dup", cmd: "bun run dup", ci: ["duplication"] },
	{ id: "site-check", area: "site", cmd: "bun run site:check", ci: ["site"], ciRun: "bun run check" },
	{ id: "site-build", area: "site", cmd: "bun run site:build", ci: ["site"], ciRun: "bun run build" },
	{
		id: "installer",
		area: "installer",
		ci: ["install-ps1"],
		ciRun: "bun test scripts/install-ps1.test.ts",
		why: "runs install.ps1 in Windows PowerShell 5.1 and PowerShell 7 on a Windows runner",
	},
];
