// Incident: the maintainer saw the OLD purple-globe Atlas icon in macOS notifications. The mark had been redrawn
// (blue, new squircle) but the brand lived in eight hand-kept copies (assets/icon.svg, favicon.svg, the studio
// logo, a stale app-icon-preview.png, rasters made by an undocumented recipe...) and nothing noticed when one
// fell behind. Now assets/logo.svg + assets/templates/* are the only designed inputs and scripts/icons.ts derives
// the rest; these tests turn a stale or hand-edited derived asset into a red `bun run check`.
import { afterEach, beforeAll, describe, expect, test } from "bun:test";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { LOCK, RASTERS, ROOT, SOURCES, drift, haveRasterTools, textOutputs } from "./icons.ts";

const abs = (p: string) => join(ROOT, p);
const edit = (p: string, f: (s: string) => string) => {
	const before = readFileSync(abs(p));
	cleanups.push(() => writeFileSync(abs(p), before));
	writeFileSync(abs(p), f(before.toString("utf8")));
};
let cleanups: (() => void)[] = [];
afterEach(() => {
	for (const c of cleanups.reverse()) c();
	cleanups = [];
});

describe("derived brand assets are up to date with assets/logo.svg", () => {
	test("committed state is clean (text outputs, source lock, rasters present)", () => {
		expect(drift({ rasters: false })).toEqual([]);
	});

	test("redrawing the mark without regenerating is reported for every consumer, not just one", () => {
		edit("assets/logo.svg", (s) => s.replace('stop-color="#3D5BFF"', 'stop-color="#FF00AA"'));
		const bad = drift({ rasters: false }).join("\n");
		for (const p of ["site/public/favicon.svg", "studio/assets/logo/logo.svg"]) expect(bad).toContain(p);
		expect(bad).toContain("assets/logo.svg: changed since the rasters were rendered");
	});

	test("a hand-edited copy of the mark is reported (the stale favicon/studio logo class of bug)", () => {
		edit("studio/assets/logo/logo.svg", (s) => s.replace("<title>Atlas</title>", "<title>Atlas old</title>"));
		expect(drift({ rasters: false }).join("\n")).toContain("studio/assets/logo/logo.svg");
	});

	test("changing a template (plate, shadow, tray) without regenerating the rasters is reported", () => {
		edit("assets/templates/app-icon.svg", (s) => s.replace("stdDeviation=\"14\"", "stdDeviation=\"30\""));
		const bad = drift({ rasters: false }).join("\n");
		expect(bad).toContain("assets/app-icon.svg");
		expect(bad).toContain("assets/templates/app-icon.svg: changed since");
	});

	test("the generator is itself part of the lock: changing the recipe makes rasters stale", () => {
		const lock = JSON.parse(readFileSync(abs(LOCK), "utf8"));
		expect(Object.keys(lock.sources).sort()).toEqual([...SOURCES, "scripts/icons.ts"].sort());
	});

	test("every derived raster is listed in the lock and exists", () => {
		const lock = JSON.parse(readFileSync(abs(LOCK), "utf8"));
		expect(Object.keys(lock.rasters).sort()).toEqual([...RASTERS].sort());
		for (const p of RASTERS) expect(existsSync(abs(p))).toBe(true);
	});

	test("no hand-made brand copies come back: only generated or designed SVG/PNG marks exist outside the allowlist", () => {
		const generated = new Set([...textOutputs().keys(), ...RASTERS, ...SOURCES]);
		const found = [...readdirSync(abs("assets"))].map((f) => `assets/${f}`).filter((f) => /\.(svg|png|ico|icns)$/.test(f));
		expect(found.filter((f) => !generated.has(f))).toEqual([]);
	});

	test("the template keeps the tray monochrome (tray.rs sets icon_as_template; macOS tints by alpha)", () => {
		const tray = textOutputs().get("assets/tray-template.svg")!;
		expect(tray).not.toMatch(/url\(#|<linearGradient|stop-color/);
		expect(tray.match(/fill="([^"]+)"/g)).toEqual(['fill="#000"']);
	});
});

// The pixel tier needs librsvg + ImageMagick + cargo-tauri (the same tools `bun run icons` uses); CI without them
// still gets the tier above. Locally it re-renders the sources and compares decoded pixels of every raster.
describe.skipIf(!haveRasterTools())("rasters match a fresh render of the sources", () => {
	let clean: string[] = [];
	beforeAll(() => {
		clean = drift({ rasters: true });
	}, 120_000);

	test("committed rasters are what the generator renders now", () => {
		expect(clean).toEqual([]);
	});

	// Incident (Linux tray): AppIndicator hosts do not tint, so the black macOS template was invisible on the default
	// dark GNOME/KDE panels. The Linux icon is the same glyph (same alpha) with white pixels.
	test("the Linux tray icon is the template glyph in white, with the same alpha", () => {
		// `txt:` lists every pixel as `x,y: (...) #RRGGBBAA ...`; the hex column is the same whatever channel layout
		// ImageMagick decoded the PNG into (this one is gray+alpha).
		const [mac, linux] = ["tray.png", "tray-linux.png"].map((f) =>
			[...Bun.spawnSync(["magick", abs(`crates/atlas-desktop/icons/${f}`), "-depth", "8", "-alpha", "on", "-colorspace", "sRGB", "txt:-"]).stdout.toString().matchAll(/#([0-9A-F]{6})([0-9A-F]{2})/g)].map((m) => ({ rgb: m[1], a: m[2] })),
		);
		expect(linux!.length).toBe(44 * 44);
		expect(linux!.map((p) => p.a)).toEqual(mac!.map((p) => p.a));
		expect(linux!.some((p) => p.a !== "00")).toBe(true);
		for (const p of linux!) if (p.a !== "00") expect(p.rgb).toBe("FFFFFF");
	});

	test("a hand-painted raster is caught by its pixels even when the sources are untouched", () => {
		const f = "site/public/apple-touch-icon.png";
		const before = readFileSync(abs(f));
		cleanups.push(() => writeFileSync(abs(f), before));
		const r = Bun.spawnSync(["magick", abs(f), "-fill", "red", "-draw", "rectangle 0,0 40,40", abs(f)]);
		expect(r.exitCode).toBe(0);
		expect(drift({ rasters: true }).join("\n")).toContain(`${f}: pixels differ from the lock`);
	}, 120_000);
});
