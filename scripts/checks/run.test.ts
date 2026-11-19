// The crate graph and the library list come from the real `cargo metadata`, and the planned commands run in a real
// (tiny) workspace: one library crate with a doctest and one bin-only crate, laid out as this repo's `crates/<name>/`.
// Incident: `cargo test -p <bin-only crate> --doc` fails with "no library targets found", and the planner emitted it for
// every change confined to atlas-desktop or atlas-cli.
import { afterAll, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { plan } from "./plan.ts";
import { workspaceMeta } from "./run.ts";

const dir = mkdtempSync(join(tmpdir(), "checks-run-"));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
const put = (f: string, text: string) => {
	mkdirSync(join(dir, f, ".."), { recursive: true });
	writeFileSync(join(dir, f), text);
};
put("Cargo.toml", '[workspace]\nresolver = "2"\nmembers = ["crates/*"]\n');
put("crates/libby/Cargo.toml", '[package]\nname = "libby"\nversion = "0.1.0"\nedition = "2021"\n');
put("crates/libby/src/lib.rs", "/// ```\n/// assert_eq!(libby::two(), 2);\n/// ```\npub fn two() -> u32 { 2 }\n");
put("crates/binny/Cargo.toml", '[package]\nname = "binny"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\nlibby = { path = "../libby" }\n');
put("crates/binny/src/main.rs", "fn main() { println!(\"{}\", libby::two()); }\n");
// A proc-macro-less third crate with both a lib and a bin: it has doctests too.
put("crates/both/Cargo.toml", '[package]\nname = "both"\nversion = "0.1.0"\nedition = "2021"\n');
put("crates/both/src/lib.rs", "pub fn one() -> u32 { 1 }\n");
put("crates/both/src/main.rs", "fn main() {}\n");

const cargo = (args: string[]) => {
	const r = Bun.spawnSync(["cargo", ...args], { cwd: dir, stdout: "pipe", stderr: "pipe", env: { ...process.env, CARGO_TARGET_DIR: join(dir, "target") } });
	return { code: r.exitCode, out: r.stdout.toString() + r.stderr.toString() };
};
const SLOW = 120_000;

describe("workspaceMeta reads the real cargo metadata", () => {
	test("the crate graph and which packages have a library target (a package with both a lib and a bin has one)", async () => {
		const m = await workspaceMeta(dir);
		expect(m.graph).toEqual({ libby: [], binny: ["libby"], both: [] });
		expect([...m.libs].sort()).toEqual(["both", "libby"]);
	}, SLOW);

	test("this repo: atlas-desktop and atlas-cli are bin-only, atlas-core is a library", async () => {
		const m = await workspaceMeta(join(import.meta.dir, "..", ".."));
		expect(m.libs).toContain("atlas-core");
		expect(m.libs).toContain("atlas-host");
		expect(m.libs).not.toContain("atlas-desktop");
		expect(m.libs).not.toContain("atlas-cli");
	}, SLOW);
});

describe("the planned doctests run where the old command failed", () => {
	test("cargo itself refuses --doc on a bin-only package (why the step must be skipped, not retried)", () => {
		const r = cargo(["test", "-p", "binny", "--doc"]);
		expect(r.code).not.toBe(0);
		expect(r.out).toContain("no library targets found");
	}, SLOW);

	test("a change in the bin-only crate plans no doctests; one in the library plans them and they pass", async () => {
		const m = await workspaceMeta(dir);
		const ofBin = plan(["crates/binny/src/main.rs"], m.graph, { libs: [...m.libs] });
		expect(ofBin.commands.map((c) => c.id)).not.toContain("doctests");
		const ofLib = plan(["crates/libby/src/lib.rs"], m.graph, { libs: [...m.libs] });
		const d = ofLib.commands.find((c) => c.id === "doctests")!;
		expect(d.cmd).toBe("cargo test -p libby --locked --doc"); // binny depends on libby but has no library
		// Every planned `cargo test` command succeeds in the real workspace (offline: no dependencies).
		for (const c of [...ofBin.commands, ...ofLib.commands].filter((x) => x.cmd.startsWith("cargo test"))) {
			const r = cargo(c.cmd.replace(/^cargo /, "").split(" ").filter((a) => a !== "--locked"));
			expect(r.code, `${c.cmd}\n${r.out}`).toBe(0);
		}
	}, SLOW);
});
