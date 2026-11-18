// @ts-nocheck bun test runtime types are not part of the site tsconfig
import { expect, test } from "bun:test";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { filmClock, filmSeconds, filmWords as spoken, FILM_DURATION, mobileFilmUi, needsRotate } from "./film";

const src = join(import.meta.dir, "..");
const chapters = JSON.parse(readFileSync(join(src, "../public/media/film-chapters.json"), "utf8"));
// The spoken length is a catalog message: these tests pin the language instead of taking whichever one an earlier test left in the runtime.
const filmWords = (s: number) => spoken(s, "en");

// Incident (2026-10-13): the heading said "78 seconds", the player's idle button "1:18" and its running clock "0:27 / 1:17"
// (floor), all typed by hand. One rule now: the TOTAL is the duration rounded to whole seconds, everything derives from it.
test("one rule for every display of the length: round to whole seconds, then format", () => {
	expect(filmSeconds(77.93)).toBe(78);
	expect(filmClock(77.93)).toBe("1:18");
	expect(filmWords(77.93)).toBe("1 minute 18 seconds");
});

test("the minute carries instead of printing 0:60 / 60 seconds", () => {
	expect(filmSeconds(59.6)).toBe(60);
	expect(filmClock(59.6)).toBe("1:00");
	expect(filmWords(59.6)).toBe("1 minute");
	expect(filmClock(59.4)).toBe("0:59");
	expect(filmWords(59.4)).toBe("59 seconds");
});

test("a re-cut changes the copy: lengths around the boundaries", () => {
	expect(filmClock(120)).toBe("2:00");
	expect(filmWords(120)).toBe("2 minutes");
	expect(filmWords(61)).toBe("1 minute 1 second");
	expect(filmClock(9.5)).toBe("0:10");
	expect(filmWords(1)).toBe("1 second");
	expect(filmWords(125)).toBe("2 minutes 5 seconds");
	expect(filmWords(66)).toBe("1 minute 6 seconds");
	expect(filmWords(181)).toBe("3 minutes 1 second");
});

test("invalid durations never print NaN", () => {
	expect(filmClock(Number.NaN)).toBe("0:00");
	expect(filmClock(-3)).toBe("0:00");
	expect(filmSeconds(Number.POSITIVE_INFINITY)).toBe(0);
});

test("FILM_DURATION is the published film-chapters.json, not a copy", () => {
	expect(FILM_DURATION).toBe(chapters.duration);
});

// The copy that names the length must not be typed by hand anywhere in the site's markup.
test("no page or component hard-codes the film's length", () => {
	const files: string[] = [];
	const walk = (d: string) => {
		for (const n of readdirSync(d)) {
			const p = join(d, n);
			if (statSync(p).isDirectory()) walk(p);
			else if (/\.(astro|svelte)$/.test(n)) files.push(p);
		}
	};
	for (const d of ["pages", "components", "layouts"]) walk(join(src, d));
	const bad = files.flatMap((f) =>
		[...readFileSync(f, "utf8").matchAll(/\b(?:6\d|7\d|8\d) seconds\b|\b1:\d\d\b|1 minute \d+ seconds/g)]
			.map((m) => `${f.split("/src/")[1]}: ${m[0]}`),
	);
	expect(bad).toEqual([]);
});

test("the published length matches the real file (skipped where the heavy media is not on disk)", async () => {
	const mp4 = join(src, "../public/media/film.mp4");
	if (!(await Bun.file(mp4).exists())) return;
	const out = Bun.spawnSync(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", mp4]).stdout.toString();
	expect(Math.abs(Number(out) - chapters.duration)).toBeLessThan(0.1);
});

// Incident (2026-10-13, phone): the 16:9 film played letterboxed in a 360px-wide box with controls cut off at the right edge.
// Until a vertical cut of the whole film exists, a portrait phone gets the native fullscreen player (landscape) and the
// inline player never grows controls it cannot fit.
test("portrait phone: tap play goes native fullscreen, with a rotate hint", () => {
	expect(mobileFilmUi({ portrait: true, width: 390, nativeFullscreen: true })).toEqual({ fullscreenOnPlay: true, rotateHint: true });
	expect(mobileFilmUi({ portrait: true, width: 820, nativeFullscreen: true }).fullscreenOnPlay).toBe(true);
});

test("landscape phone, tablet-wide and desktop play inline", () => {
	expect(mobileFilmUi({ portrait: false, width: 844, nativeFullscreen: true })).toEqual({ fullscreenOnPlay: false, rotateHint: false });
	expect(mobileFilmUi({ portrait: true, width: 1024, nativeFullscreen: true })).toEqual({ fullscreenOnPlay: false, rotateHint: false });
	expect(mobileFilmUi({ portrait: false, width: 1440, nativeFullscreen: false })).toEqual({ fullscreenOnPlay: false, rotateHint: false });
});

test("a portrait phone whose browser has no fullscreen API still gets the rotate hint, not a letterbox without a word", () => {
	expect(mobileFilmUi({ portrait: true, width: 390, nativeFullscreen: false })).toEqual({ fullscreenOnPlay: false, rotateHint: true });
});

// After the hand-off to fullscreen, a portrait screen that could not be locked to landscape still letterboxes a 16:9 film:
// the player must say so (a rotate prompt inside the player), and say nothing once the screen is landscape.
test("rotate prompt: shown while fullscreen is portrait, gone once landscape or when not fullscreen", () => {
	expect(needsRotate({ fullscreen: true, portrait: true })).toBe(true);
	expect(needsRotate({ fullscreen: true, portrait: false })).toBe(false);
	expect(needsRotate({ fullscreen: false, portrait: true })).toBe(false);
});
