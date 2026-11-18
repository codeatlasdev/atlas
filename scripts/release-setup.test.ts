// #317: release.yml set up Rust 6 times, installed apt packages 5 times and exported SOURCE_DATE_EPOCH 9 times, each copy
// free to drift (a build with another epoch is a build that is not the same bytes). One composite action does all three.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const read = (f: string) => readFileSync(join(root, f), "utf8");
const text = read(".github/workflows/release.yml");
const release = Bun.YAML.parse(text) as any;
const action = Bun.YAML.parse(read(".github/actions/release-toolchain/action.yml")) as any;

describe("release.yml shares one toolchain setup", () => {
	test("the Rust install and the epoch live in the action, once, and nowhere in the workflow", () => {
		expect(text).not.toContain("rustup");
		expect(text).not.toContain("SOURCE_DATE_EPOCH");
		expect(text).not.toContain("$SUDO apt-get"); // the container matrix of verify-linux installs with its own distro tool
		const runs = action.runs.steps.map((s: any) => s.run ?? "").join("\n");
		expect(runs.match(/SOURCE_DATE_EPOCH=/g)?.length).toBe(1);
		expect(runs.match(/sh\.rustup\.rs/g)?.length).toBe(1);
		// The epoch is the commit's time and reaches every later step of the job.
		expect(runs).toContain('git log -1 --format=%ct)" >> "$GITHUB_ENV"');
	});

	test("every job that compiles uses the action after its checkout", () => {
		for (const [name, job] of Object.entries<any>(release.jobs)) {
			const steps: any[] = job.steps ?? [];
			const compiles = steps.some((s) => /\bcargo (build|xwin build|tauri|install tauri)/.test(s.run ?? ""));
			if (!compiles) continue;
			const setup = steps.findIndex((s) => s.uses === "./.github/actions/release-toolchain");
			const checkout = steps.findIndex((s) => String(s.uses ?? "").startsWith("actions/checkout"));
			expect(setup, `${name} sets up the toolchain`).toBeGreaterThan(checkout);
		}
	});

	test("the cross builds still get their Windows target and the Linux builds their packages", () => {
		const with_ = (job: string) => release.jobs[job].steps.find((s: any) => s.uses === "./.github/actions/release-toolchain").with ?? {};
		for (const j of ["windows-binaries", "windows-installer", "windows-setup"]) expect(with_(j).targets, j).toBe("x86_64-pc-windows-msvc");
		expect(with_("build-linux").apt).toContain("libwebkit2gtk-4.1-dev");
		expect(with_("windows-installer").apt).toContain("nsis");
		expect(with_("windows-binaries").apt).toContain("clang");
	});

	test("the action takes its inputs through env, never spliced into a script", () => {
		for (const s of action.runs.steps) expect(s.run ?? "").not.toContain("${{");
	});
});
