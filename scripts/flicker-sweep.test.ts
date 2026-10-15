// Incident: the site videos showed flicker the app's own checks never saw. The sweep is the guard; these tests run the
// REAL script as a process (no app needed) and check what a release relies on: that a flickering recording exits 1 with a
// timestamp, that a clean one exits 0, and that the list of surfaces stays complete. Recordings are real ffmpeg output.
import { describe, expect, test } from "bun:test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fixtures } from "./flicker-sweep-fixtures.ts";
import { SURFACES } from "./flicker-sweep-surfaces.ts";

const script = join(import.meta.dir, "flicker-sweep.ts");
const dir = mkdtempSync(join(tmpdir(), "flicker-sweep-test-"));

/** A 60 fps 640x400 clip: a textured "app" (gradient + a bar) for `n` frames, with frame `blip` (if any) replaced by `blipFilter`. */
async function clip(name: string, n: number, blip?: { at: number; draw: string; full?: boolean }): Promise<string> {
	const out = join(dir, `${name}.mov`);
	const base = "gradients=s=640x400:r=60:c0=0x203040:c1=0x405060:nb_colors=2:speed=0.0001";
	const enable = blip ? `between(n\\,${blip.at}\\,${blip.at})` : "0";
	const draw = blip ? `,drawbox=x=${blip.full ? 0 : 100}:y=${blip.full ? 0 : 60}:w=${blip.full ? 640 : 300}:h=${blip.full ? 400 : 200}:color=${blip.draw}:t=fill:enable='${enable}'` : "";
	const r = Bun.spawnSync(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", base, "-frames:v", String(n), "-vf", `drawbox=x=20:y=20:w=80:h=360:color=0x806040@1:t=fill${draw}`, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "12", out]);
	expect(r.exitCode).toBe(0);
	return out;
}
const judge = (file: string, surface = "t") => {
	const r = Bun.spawnSync(["bun", script, "--judge", file, "--surface", surface, "--out", dir], { stdout: "pipe", stderr: "pipe" });
	return { code: r.exitCode, err: r.stderr.toString(), out: r.stdout.toString() };
};

describe("--judge on a real recording", () => {
	test("a steady page exits 0", async () => {
		const r = judge(await clip("clean", 90));
		expect(r.code).toBe(0);
		expect(JSON.parse(r.out).blips).toEqual([]);
	});

	test("a region that flashes bright for one frame exits 1 and names the surface and the second", async () => {
		const r = judge(await clip("blip", 90, { at: 45, draw: "white" }), "settings-open-close");
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/settings-open-close: blip at 0\.7\d\d s/);
	});

	test("a black frame in the middle exits 1 as a blank frame", async () => {
		const r = judge(await clip("black", 90, { at: 30, draw: "black", full: true }));
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/black frame at 0\.\d+ s/);
	});

	test("a whole-frame strobe (8 flips in a second) exits 1 as luminance flashing, judged by the studio's own BT.1702 rule", async () => {
		const out = join(dir, "strobe.mov");
		// alternate black/white every 3 frames (10 Hz) over 90 frames
		const r = Bun.spawnSync(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", "color=c=white:s=640x400:r=60", "-frames:v", "90", "-vf", "drawbox=x=0:y=0:w=640:h=400:color=0x181818:t=fill:enable='lt(mod(n\\,6)\\,3)'", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "12", out]);
		expect(r.exitCode).toBe(0);
		const j = judge(out, "strobe");
		expect(j.code).toBe(1);
		expect(j.err).toMatch(/strobe: luminance flashing/);
	});

	test("a file with fewer than 3 frames is an error, not a pass", async () => {
		const r = judge(await clip("short", 2));
		expect(r.code).toBe(1);
		expect(r.err).toMatch(/frames/);
	});
});

describe("the surface list", () => {
	test("every surface is unique, records longer than 3 s and says what it covers", () => {
		const ids = SURFACES.map((s) => s.id);
		expect(new Set(ids).size).toBe(ids.length);
		for (const s of SURFACES) {
			expect(s.seconds).toBeGreaterThanOrEqual(4);
			expect(s.covers.length).toBeGreaterThan(10);
		}
	});

	// Incident: a surface was filmed with no state to show (the Panel with no services) and passed. A surface that claims to
	// show something must say how to tell it is showing it.
	test("the surfaces whose point is content on screen declare what must be visible (`expects`)", () => {
		const need = ["panel-services", "panel-action-toast", "logs", "changes", "settings", "terminal", "dialogs", "toast-error"];
		for (const id of need) {
			const s = SURFACES.find((x) => x.id === id)!;
			expect(s.expects, `${id} must declare expects`).toBeTruthy();
			expect(s.expects!.length).toBeGreaterThan(0);
		}
	});

	// The user's bar is "no flicker anywhere": dropping a surface silently would hide one. Update this list on purpose.
	test("covers every area the sweep was asked to", () => {
		expect(SURFACES.map((s) => s.id)).toEqual([
			"boot",
			"sidebar-select",
			"space-switch",
			"panel-services",
			"panel-action-toast",
			"logs",
			"changes",
			"settings",
			"dialogs",
			"menus-tooltips",
			"palette",
			"toast-error",
			"terminal",
			"terminal-resize",
			"browser",
			"tabs",
		]);
	});
});

describe("the fixture projects", () => {
	// Incident (first full run): the fixture's action used key `m`, a built-in dashboard key, so atlas.yaml was invalid, the
	// stack never started and the "Panel" surface was filmed with no services at all while reporting a clean pass.
	test("every fixture atlas.yaml passes the real validator (`atlas config`)", () => {
		const fx = fixtures();
		try {
			for (const root of fx.roots) {
				const r = Bun.spawnSync([join(import.meta.dir, "../target/debug/atlas"), "config", "check", "--dir", root], { stdout: "pipe", stderr: "pipe" });
				expect({ root, code: r.exitCode, err: r.stderr.toString() + r.stdout.toString() }).toMatchObject({ root, code: 0 });
			}
		} finally {
			fx.cleanup();
		}
	});
});

describe("fixtures.cleanup leaves no process behind", () => {
	// Incident (three full sweep runs): the app starts each fixture stack as a detached `atlas dev --runner` (ppid 1). Stopping
	// the agent instance does not stop them, so every run leaked a runner with its services. cleanup() must `atlas down`.
	const atlas = join(import.meta.dir, "../target/debug/atlas");
	const alive = (root: string) => Bun.spawnSync(["pgrep", "-f", "--", `dev --runner --dir ${root}`]).stdout.toString().trim().split("\n").filter(Boolean);

	test("a running fixture stack is stopped by cleanup()", async () => {
		const fx = fixtures();
		const root = fx.roots[1]!; // sweep-docs: one small service
		const up = Bun.spawnSync([atlas, "dev", "--detach", "--dir", root], { stdout: "pipe", stderr: "pipe" });
		expect(up.exitCode, up.stderr.toString()).toBe(0);
		for (let i = 0; i < 40 && alive(root).length === 0; i++) await Bun.sleep(250);
		expect(alive(root).length, "the runner should be up before cleanup").toBeGreaterThan(0);
		fx.cleanup();
		for (let i = 0; i < 40 && alive(root).length > 0; i++) await Bun.sleep(250);
		expect(alive(root)).toEqual([]);
	}, 30_000);
});
