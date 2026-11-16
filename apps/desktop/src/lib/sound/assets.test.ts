// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { CATEGORIES } from "./policy.ts";

const DIR = join(import.meta.dir, "../../../static/sounds");

// Regression (#265): only AAC .m4a shipped, and WebKitGTK on a GStreamer without an AAC decoder (the AppImage's
// Ubuntu plugin set, a bare Arch install) cannot decode it, so Linux had no sound. The engine falls back to .ogg
// (Vorbis, decoded by plugins-base), which therefore has to exist for every category the engine can play.
describe("earcon files", () => {
	test("every category ships an AAC .m4a and a Vorbis .ogg,", () => {
		for (const c of CATEGORIES) {
			expect(existsSync(join(DIR, `${c}.m4a`)), `${c}.m4a`).toBe(true);
			expect(existsSync(join(DIR, `${c}.ogg`)), `${c}.ogg`).toBe(true);
		}
	});

	test("each .ogg is a real Ogg Vorbis stream (the codec the fallback relies on)", () => {
		for (const c of CATEGORIES) {
			const b = readFileSync(join(DIR, `${c}.ogg`));
			expect(b.subarray(0, 4).toString("latin1"), `${c}: Ogg page`).toBe("OggS");
			// First packet of the first page is the Vorbis identification header: 0x01 "vorbis".
			expect(b.subarray(28, 35).toString("latin1"), `${c}: codec`).toBe("\x01vorbis");
			expect(b.length, `${c}: size budget`).toBeLessThan(20 * 1024);
		}
	});
});
