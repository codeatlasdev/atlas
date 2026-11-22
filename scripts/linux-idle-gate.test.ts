// scripts/linux-idle-gate.sh sends keyboard input (wtype) and window commands (swaymsg) with whatever display variables the
// shell holds. A variable pointing at the user's session would put a keypress on his screen, so before any input the gate
// asserts, through `headless_assert_private`, that XDG_RUNTIME_DIR, the Wayland socket and SWAYSOCK are all inside its own
// work dir (a sane run is shown by the gate itself in CI; here the refusals, which no real run can show).
import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";

const lib = join(import.meta.dir, "linux-headless.sh");

/** A work dir shaped like headless_prepare's: `run/` with a real unix socket `wayland-1` and a `sway-ipc.sock` file. */
async function work() {
	const w = mkdtempSync(join(tmpdir(), "gate-guard-"));
	mkdirSync(join(w, "run"));
	await new Promise<void>((ok) => createServer().listen(join(w, "run", "wayland-1"), ok).unref());
	writeFileSync(join(w, "run", "sway-ipc.sock"), "");
	return w;
}

function guard(w: string, env: Record<string, string>) {
	const r = Bun.spawnSync(["bash", "-c", `. ${lib}; work=${w}; headless_assert_private`], { env: { PATH: process.env.PATH ?? "", ...env }, stderr: "pipe" });
	return { code: r.exitCode, err: r.stderr.toString() };
}

describe("the gate sends input only to its own private display", () => {
	test("its own runtime dir, Wayland socket and sway socket pass", async () => {
		const w = await work();
		expect(guard(w, { XDG_RUNTIME_DIR: join(w, "run"), WAYLAND_DISPLAY: "wayland-1", SWAYSOCK: join(w, "run", "sway-ipc.sock") }).code).toBe(0);
	});
	test("the user's runtime dir is refused with exit 3", async () => {
		const w = await work();
		const r = guard(w, { XDG_RUNTIME_DIR: `/run/user/${process.getuid?.() ?? 1000}`, WAYLAND_DISPLAY: "wayland-1", SWAYSOCK: join(w, "run", "sway-ipc.sock") });
		expect(r.code).toBe(3);
		expect(r.err).toContain("XDG_RUNTIME_DIR");
	});
	test("a Wayland socket that is not inside the runtime dir is refused (an absolute path or a missing socket)", async () => {
		const w = await work();
		const base = { XDG_RUNTIME_DIR: join(w, "run"), SWAYSOCK: join(w, "run", "sway-ipc.sock") };
		expect(guard(w, { ...base, WAYLAND_DISPLAY: "/run/user/1000/wayland-1" }).code).toBe(3);
		expect(guard(w, { ...base, WAYLAND_DISPLAY: "wayland-9" }).code).toBe(3);
		expect(guard(w, base).code).toBe(3);
	});
	test("a SWAYSOCK outside the work dir (the user's sway) is refused", async () => {
		const w = await work();
		const r = guard(w, { XDG_RUNTIME_DIR: join(w, "run"), WAYLAND_DISPLAY: "wayland-1", SWAYSOCK: "/run/user/1000/sway-ipc.1000.1.sock" });
		expect(r.code).toBe(3);
		expect(r.err).toContain("SWAYSOCK");
	});
	test("a runtime dir that merely shares the work dir as a string prefix is refused", async () => {
		const w = await work();
		const r = guard(w, { XDG_RUNTIME_DIR: `${w}run-evil`, WAYLAND_DISPLAY: "wayland-1", SWAYSOCK: join(w, "run", "sway-ipc.sock") });
		expect(r.code).toBe(3);
	});
});
