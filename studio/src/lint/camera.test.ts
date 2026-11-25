import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { moveSeconds, planCamera, poseAt, toOutput } from "../camera/plan.ts";
import type { CameraKey, CameraSidecar, CaptionCue, Frame, ResolvedFocus, ShotPlan } from "../camera/types.ts";
import { DEFAULT_LIMITS } from "../camera/types.ts";
import { StudioError } from "../lib/paths.ts";
import { run } from "../lib/exec.ts";
import { checkCameraSpeed, checkCaptionSafeArea, checkCaptionText, checkCameraSubjects, checkFocusLate, checkFocusText, frameKindAt, loadSidecar, outsideViewports, sampleTimes, scanFocuses, viewportsAt } from "./camera.ts";
import { ocr, type TextBox } from "./ocr.ts";
import { PROFILES } from "./profiles.ts";

const vertical = PROFILES["social-vertical"]!;
const square = PROFILES["social-square"]!;

let dir: string;
beforeAll(() => {
	dir = mkdtempSync(join(tmpdir(), "studio-lint-camera-"));
});
afterAll(() => {
	if (dir.startsWith(tmpdir())) rmSync(dir, { recursive: true, force: true });
});

// A 2x retina app recording (1600x1000 source px) shown in the middle of a 1080x1920 frame.
const frame: Frame = { source: { w: 1600, h: 1000 }, viewport: { x: 0, y: 460, w: 1080, h: 1000 }, frame: { w: 1080, h: 1920 }, scale: 2 };
const legibility = { minTextPx: vertical.minFocusTextPx!, displayHeight: vertical.displayHeight };

const focusAt = (id: string, at: number, x: number, y: number, textPx = 12, w = 240, h = 40): ResolvedFocus => ({ id, at, rect: { x, y, w, h }, via: "rect", textPx });

/** A sidecar whose shot is the REAL planner's output: nothing about the keys is typed by hand. */
function sidecarOf(focuses: ResolvedFocus[], opts: { frame?: Frame; limits?: typeof DEFAULT_LIMITS; duration?: number; start?: number; zoomOverrides?: Record<string, number> } = {}): CameraSidecar {
	const f = opts.frame ?? frame;
	const limits = opts.limits ?? DEFAULT_LIMITS;
	const start = opts.start ?? 0;
	const { keys, report } = planCamera(f, legibility, limits, focuses, opts.duration ?? 8, { zoomOverrides: opts.zoomOverrides });
	const shot: ShotPlan = {
		id: "s1",
		start,
		end: start + (opts.duration ?? 8),
		viewport: f.viewport,
		source: f.source,
		scale: f.scale,
		keys: keys.map((k) => ({ ...k, t: k.t })),
		focuses: report.map((r) => ({ id: r.id, from: start + r.from, to: start + r.to, rect: r.outRect, zoom: r.zoom, neededZoom: r.neededZoom, shownTextPx: r.shownTextPx, textPx: r.textPx })),
		limits,
	};
	return { version: 1, film: "t", cut: "c", aspect: "9:16", frame: f.frame, fps: 30, shots: [shot] };
}

const withShot = (sc: CameraSidecar, patch: Partial<ShotPlan>): CameraSidecar => ({ ...sc, shots: [{ ...sc.shots[0]!, ...patch }, ...sc.shots.slice(1)] });

describe("sidecar loading", () => {
	test("absent -> undefined, valid -> parsed, malformed -> StudioError", () => {
		const f = join(dir, "a.mp4");
		expect(loadSidecar(f)).toBeUndefined();
		const sc = sidecarOf([focusAt("f1", 2, 800, 500)]);
		writeFileSync(join(dir, "a.camera.json"), JSON.stringify(sc));
		expect(loadSidecar(f)?.shots[0]?.id).toBe("s1");
		writeFileSync(join(dir, "b.camera.json"), JSON.stringify({ ...sc, version: 2 }));
		expect(() => loadSidecar(join(dir, "b.mp4"))).toThrow(StudioError);
		writeFileSync(join(dir, "c.camera.json"), "{not json");
		expect(() => loadSidecar(join(dir, "c.mp4"))).toThrow(StudioError);
		writeFileSync(join(dir, "d.camera.json"), JSON.stringify({ version: 1, shots: [{ id: "x" }] }));
		expect(() => loadSidecar(join(dir, "d.mp4"))).toThrow(StudioError);
	});
});

describe("camera/too-fast", () => {
	// two focuses 200 source px apart at the same zoom: ~400 output px, a pan (the planner CUTS when a move would need
	// more than maxMoveSeconds or crosses cutDistancePx)
	const planned = sidecarOf([focusAt("a", 1.5, 300, 300), focusAt("b", 4, 500, 300)]);
	test("the fixture is what the tests below assume: keys[2] is a real pan at constant zoom", () => {
		const k = planned.shots[0]!.keys;
		expect(k[2]!.move).toBeGreaterThan(0);
		expect(k[2]!.ease).toBe("ease");
		expect(k[2]!.zoom).toBe(k[1]!.zoom);
	});

	test("the real planner's output passes", () => {
		expect(checkCameraSpeed(planned)).toEqual([]);
	});

	test("a move squeezed into a third of the time is an error naming shot, measured and limit, with a range in CUT seconds", () => {
		const keys = planned.shots[0]!.keys.map((k, i): CameraKey => (i === 2 ? { ...k, move: k.move / 3 } : k));
		const sc = withShot({ ...planned, shots: [{ ...planned.shots[0]!, start: 10, end: 18 }] }, { keys });
		const d = checkCameraSpeed(sc);
		expect(d.length).toBe(1);
		const x = d[0]!;
		expect(x.severity).toBe("error");
		expect(x.code).toBe("camera/too-fast");
		expect(x.message).toContain("s1");
		expect(x.measured!).toBeGreaterThan(x.limit!);
		expect(x.limit).toBe(DEFAULT_LIMITS.maxPanPxPerSec);
		const k = keys[2]!;
		expect(x.range!.start).toBeGreaterThanOrEqual(10 + k.t - 0.01); // shot.start + t, rounded to 10 ms
		expect(x.range!.end).toBeLessThanOrEqual(10 + k.t + k.move + 0.01);
		expect(x.fix?.action).toBe("slow-camera");
		expect(x.fix?.params?.shot).toBe("s1");
	});

	test("applying minMoveSeconds to the violating move makes the shot pass (lint -> fix -> lint)", () => {
		const keys = planned.shots[0]!.keys.map((k, i): CameraKey => (i === 2 ? { ...k, move: k.move / 3 } : k));
		const bad = withShot(planned, { keys });
		const fix = checkCameraSpeed(bad)[0]!.fix!;
		const fixed = keys.map((k, i): CameraKey => (i === 2 ? { ...k, move: Number(fix.params!.minMoveSeconds) } : k));
		expect(checkCameraSpeed(withShot(planned, { keys: fixed }))).toEqual([]);
	});

	test("zoom rate over the limit is its own error (e-folds/s)", () => {
		const keys = planned.shots[0]!.keys.map((k, i): CameraKey => (i === 1 ? { ...k, move: k.move / 4 } : k));
		const d = checkCameraSpeed(withShot(planned, { keys }));
		expect(d.some((x) => x.code === "camera/too-fast" && x.limit === DEFAULT_LIMITS.maxZoomPerSec)).toBe(true);
	});

	test("boundary: a move planned at exactly the limit passes; 5% tolerance admits 4% over and rejects 20% over", () => {
		// a pure pan at constant zoom: peak speed = 1.875 * distance / move (smootherstep)
		const mk = (move: number): CameraSidecar => {
			const keys: CameraKey[] = [
				{ t: 0, x: 400, y: 500, zoom: 1, ease: "hold", move: 0 },
				{ t: 1, x: 1200, y: 500, zoom: 1, ease: "ease", move },
			];
			return withShot(planned, { keys, focuses: [] });
		};
		const exact = moveSeconds(frame, DEFAULT_LIMITS, { x: 400, y: 500, zoom: 1 }, { x: 1200, y: 500, zoom: 1 });
		expect(checkCameraSpeed(mk(exact))).toEqual([]);
		expect(checkCameraSpeed(mk(exact / 1.04))).toEqual([]);
		expect(checkCameraSpeed(mk(exact / 1.2)).length).toBe(1);
	});

	test("a cut (move 0) is not motion", () => {
		const keys: CameraKey[] = [
			{ t: 0, x: 400, y: 500, zoom: 1, ease: "hold", move: 0 },
			{ t: 1, x: 1200, y: 500, zoom: 2, ease: "cut", move: 0 },
		];
		expect(checkCameraSpeed(withShot(planned, { keys, focuses: [] }))).toEqual([]);
	});

	test("shots without keys are skipped", () => {
		expect(checkCameraSpeed(withShot(planned, { keys: [], viewport: undefined }))).toEqual([]);
	});
});

describe("camera/subject-out-of-frame", () => {
	// vertical safe rect: x 65..(1080-151), y 269..(1920-384): the viewport (y 460..1460) sits inside it
	test("a focus planned by the planner is inside viewport and safe area", () => {
		expect(checkCameraSubjects(sidecarOf([focusAt("a", 2, 800, 500)]), vertical)).toEqual([]);
	});

	test("a wide focus planned without overrides is zoomed out by the planner until it fits the viewport: no viewport diagnostic", () => {
		const d = checkCameraSubjects(sidecarOf([focusAt("wide", 2, 800, 500, 12, 700, 40)]), vertical);
		expect(d.filter((x) => x.message.includes("viewport"))).toEqual([]);
	});

	test("a rect wider than the viewport at the hold zoom (a zoom override the rect cannot fit): error naming the side and px, with the largest zoom that fits", () => {
		// the planner itself zooms a wide focus out until it fits; `studio fix` overrides can force a zoom it does not:
		// 700 source px at zoom 2.9 is ~1370 output px, wider than the 1080 viewport
		const sc = sidecarOf([focusAt("wide", 2, 800, 500, 12, 700, 40)], { zoomOverrides: { wide: 2.9 } });
		const d = checkCameraSubjects(sc, vertical).filter((x) => x.fix?.action === "set-camera-zoom");
		expect(d.length).toBe(1);
		const x = d[0]!;
		expect(x.severity).toBe("error");
		expect(x.code).toBe("camera/subject-out-of-frame");
		expect(x.message).toMatch(/left|right/);
		expect(x.message).toMatch(/\d+px/);
		const zoom = Number(x.fix!.params!.zoom);
		expect(zoom).toBeLessThan(sc.shots[0]!.focuses[0]!.zoom);
		expect(x.fix!.params!.focus).toBe("wide");
		// re-derive: at that zoom the rect fits the viewport width
		const shot = sc.shots[0]!;
		const fit = shot.viewport!.w / shot.source!.w;
		expect(700 * fit * zoom).toBeLessThanOrEqual(shot.viewport!.w + 1);
		expect(700 * fit * (zoom + 0.05)).toBeGreaterThan(shot.viewport!.w - 1);
	});

	test("a rect that fits the viewport but pokes into the platform UI zone is an error that names the side (no zoom fix)", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = sc.shots[0]!;
		// a viewport that starts under the top UI bar: the focus is inside the viewport but outside the safe rect
		const vp = { x: 0, y: 100, w: 1080, h: 1000 };
		const moved = withShot(sc, { viewport: vp, focuses: [{ ...shot.focuses[0]!, rect: { x: 200, y: 150, w: 300, h: 60 } }] });
		const d = checkCameraSubjects(moved, vertical);
		expect(d.length).toBe(1);
		expect(d[0]!.message).toContain("top");
		expect(d[0]!.fix?.action).toBe("move-out-of-safe-zone");
	});

	test("boundary: a rect exactly on the safe edge passes, one px over fails", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = sc.shots[0]!;
		const wide = { ...shot, viewport: { x: 0, y: 0, w: 1080, h: 1920 } };
		const edge = { x: 65, y: 269, w: 100, h: 100 }; // safe.x = round(1080*.06)=65, safe.y = round(1920*.14)=269
		const at = (r: typeof edge) => checkCameraSubjects({ ...sc, shots: [{ ...wide, focuses: [{ ...shot.focuses[0]!, rect: r }] }] }, vertical);
		expect(at(edge)).toEqual([]);
		expect(at({ ...edge, x: 64 }).length).toBe(1);
	});

	test("recomputes from the keys: a camera still moving while the text must be read is caught", () => {
		// the focus is 500 source px off-center; its move is stretched to 6 s so at `from` the camera is mid-flight
		const sc = sidecarOf([focusAt("b", 3, 1400, 800)]);
		const shot = sc.shots[0]!;
		const keys = shot.keys.map((k, i) => (i === 1 ? { ...k, move: 6 } : k));
		const d = checkCameraSubjects(withShot(sc, { keys }), vertical);
		expect(d.some((x) => x.code === "camera/subject-out-of-frame" && x.message.includes("viewport"))).toBe(true);
		// independent geometry: where the source rect really is at `from` with those keys
		const out = toOutput({ source: shot.source!, viewport: shot.viewport!, frame: sc.frame, scale: shot.scale! }, poseAt(keys, shot.focuses[0]!.from - shot.start), { x: 1400, y: 800, w: 240, h: 40 });
		const vp = shot.viewport!;
		expect(out.x + out.w > vp.x + vp.w || out.x < vp.x || out.y + out.h > vp.y + vp.h || out.y < vp.y).toBe(true);
	});

	test("no viewport: only the safe rect is checked", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = sc.shots[0]!;
		const d = checkCameraSubjects({ ...sc, shots: [{ ...shot, viewport: undefined, focuses: [{ ...shot.focuses[0]!, rect: { x: 0, y: 0, w: 50, h: 50 } }] }] }, vertical);
		expect(d.length).toBe(1);
	});
});

const sidecar0 = sidecarOf([focusAt("a", 2, 800, 500)]);

const box = (text: string, h: number, rect: { x: number; y: number; w: number; h: number }, conf = 0.9): TextBox => ({ text, rect: { ...rect, h }, conf });

describe("text/too-small-for-target (injected OCR)", () => {
	// textPx 16: the legibility zoom (2.37) is below the 2x-resolution cap (2.96), so a fix can still ask for more
	const sc = sidecarOf([focusAt("a", 2, 800, 500, 16)]);
	const fp = sc.shots[0]!.focuses[0]!;
	const inside = { x: fp.rect.x + 10, y: fp.rect.y + 5, w: 100, h: 20 };
	const bandFor = (shown: number) => (shown * sc.frame.h) / vertical.displayHeight;
	const run1 = (boxes: TextBox[]) => checkFocusText(sc, vertical, [{ shot: sc.shots[0]!, focus: fp, boxes }]);

	test("glyph band shown clearly above the minimum passes, and the limit itself passes", () => {
		expect(run1([box("Settings", bandFor(18.5), inside)])).toEqual([]);
		// exactly the limit: band x display / frameH lands a hair under 18 in floating point, which is not a defect
		expect(run1([box("Settings", bandFor(18), inside)])).toEqual([]);
	});

	test("below min but above 0.6*min is a warning with a set-camera-zoom fix: zoom x (limit/shown) x 1.05, within the resolution cap", () => {
		const d = run1([box("Settings", bandFor(17), inside)]);
		expect(d.length).toBe(1);
		const x = d[0]!;
		expect(x.severity).toBe("warning");
		expect(x.code).toBe("text/too-small-for-target");
		expect(x.measured).toBeCloseTo(17, 1);
		expect(x.limit).toBe(18);
		expect(x.range).toEqual({ start: Math.round(fp.from * 100) / 100, end: Math.round(fp.to * 100) / 100 });
		expect(x.rect).toEqual({ ...inside, h: bandFor(17) });
		expect(x.fix!.action).toBe("set-camera-zoom");
		expect(Number(x.fix!.params!.zoom)).toBeCloseTo(Math.round(fp.zoom * (18 / 17) * 1.05 * 100) / 100, 2);
		expect(x.fix!.params!.maxZoom).toBeCloseTo(2 / (sc.shots[0]!.viewport!.w / sc.shots[0]!.source!.w), 2);
	});

	test("below 0.6*min is an error, 0.6*min and a hair above is a warning", () => {
		expect(run1([box("Settings", bandFor(10), inside)])[0]!.severity).toBe("error");
		expect(run1([box("Settings", bandFor(18 * 0.6 - 0.1), inside)])[0]!.severity).toBe("error");
		expect(run1([box("Settings", bandFor(18 * 0.6 + 0.1), inside)])[0]!.severity).toBe("warning");
	});

	test("when the zoom needed exceeds the resolution cap the fix is enlarge-source-text and it stays an error", () => {
		const d = run1([box("Settings", bandFor(5), inside)]);
		expect(d[0]!.severity).toBe("error");
		expect(d[0]!.fix!.action).toBe("enlarge-source-text");
		expect(Number(d[0]!.fix!.params!.neededZoom)).toBeGreaterThan(Number(d[0]!.fix!.params!.maxZoom));
	});

	test("only the SMALLEST band among confident, 3+ char boxes counts; low-confidence and short boxes are ignored", () => {
		// every box starts at the focus' top-left and is at most 30 shown px (68 px) tall: all of them lie inside the grown rect
		const top = { x: fp.rect.x, y: fp.rect.y, w: 100, h: 0 };
		const d = run1([box("Settings", bandFor(30), top), box("ab", bandFor(2), top), box("Noise", bandFor(2), top, 0.3), box("Footnote", bandFor(15), top)]);
		expect(d.length).toBe(1);
		expect(d[0]!.measured).toBeCloseTo(15, 1);
	});

	test("boxes outside the focus rect grown by 12% are ignored; boxes inside the growth margin count", () => {
		const far = { x: fp.rect.x + fp.rect.w + fp.rect.w * 0.2, y: fp.rect.y, w: 50, h: 20 };
		expect(run1([box("Elsewhere", bandFor(5), far)])).toEqual([expect.objectContaining({ code: "text/focus-empty" })]);
		const margin = { x: fp.rect.x + fp.rect.w, y: fp.rect.y, w: fp.rect.w * 0.1, h: 20 };
		expect(run1([box("Margin", bandFor(15), margin)])[0]!.code).toBe("text/too-small-for-target");
	});

	test("no text found in a focus that declared textPx is the warning text/focus-empty", () => {
		const d = run1([]);
		expect(d.length).toBe(1);
		expect(d[0]!.severity).toBe("warning");
		expect(d[0]!.code).toBe("text/focus-empty");
		expect(d[0]!.range).toEqual({ start: Math.round(fp.from * 100) / 100, end: Math.round(fp.to * 100) / 100 });
	});

	test("a profile without minFocusTextPx has no focus rule", () => {
		expect(checkFocusText(sc, { ...vertical, minFocusTextPx: undefined }, [{ shot: sc.shots[0]!, focus: fp, boxes: [] }])).toEqual([]);
	});
});

describe("text/too-small-for-target on real OCR of a real rendered image", () => {
	// A 1080x1920 PNG painted with ImageMagick (this machine's ffmpeg is built without drawtext), measured by the real OCR
	// engine (Vision on macOS, tesseract when installed). Measured on this machine: Vision reads nothing below ~20 px,
	// a 24 px font has a 22 px glyph band (9.7 px shown at 844/1920: under 0.6 x 18 -> error) and a 48 px font a 51 px
	// band (22.4 px shown: passes). A 40 px font measures 37 px (16.3 shown), which is below the 18 px minimum.
	const font = "/System/Library/Fonts/Supplemental/Arial.ttf";
	const paint = async (name: string, size: number) => {
		const png = join(dir, name);
		await run(["magick", "-size", "1080x1920", "xc:white", "-font", font, "-pointsize", String(size), "-fill", "black", "-annotate", "+100+900", "Quarterly revenue report", png]);
		return png;
	};

	test("24 px text is flagged and 48 px text passes", async () => {
		const small = await paint("small.png", 24);
		const big = await paint("big.png", 48);
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const fp = { ...sc.shots[0]!.focuses[0]!, rect: { x: 60, y: 840, w: 700, h: 120 } };
		const shot = { ...sc.shots[0]!, focuses: [fp] };
		const scan = async (png: string) => {
			const { boxes } = await ocr(png);
			return checkFocusText({ ...sc, shots: [shot] }, vertical, [{ shot, focus: fp, boxes }]);
		};
		const bad = await scan(small);
		expect(bad.map((x) => x.code)).toContain("text/too-small-for-target");
		expect(bad.find((x) => x.code === "text/too-small-for-target")!.severity).toBe("error");
		expect(await scan(big)).toEqual([]);
		// 12 px text: Vision reads nothing that small, so the focus reads as empty; tesseract 5.5 reads it and measures it
		// too small (both seen on macOS, 2026-11-24). Each engine has its exact finding, an error either way.
		const tinyPng = await paint("tiny.png", 12);
		const { engine } = await ocr(tinyPng);
		const tiny = await scan(tinyPng);
		expect(tiny.map((x) => [x.code, x.severity])).toEqual([[engine === "vision" ? "text/focus-empty" : "text/too-small-for-target", "error"]]);
	}, 60_000);
});
describe("camera/focus-late", () => {
	// five focuses 0.5 s apart in a 4 s shot: the camera needs ~2.3 s for the first move, then ~1.2 s hold + ~1.1 s move
	// for each next one, so the later ones are planned past the end of the shot (the planner reports them, never rushes)
	const crowded = (duration: number) => sidecarOf([0, 1, 2, 3, 4].map((i) => focusAt(`f${i}`, 0.5 + i * 0.5, 300 + i * 150, 300)), { duration });
	const sc = crowded(4);
	const late = sc.shots[0]!.focuses.filter((f) => f.to > sc.shots[0]!.end + 0.01);

	test("the fixture is what the tests assume: the planner itself plans some focuses past the shot's end, some inside it", () => {
		expect(late.length).toBeGreaterThan(0);
		expect(late.length).toBeLessThan(sc.shots[0]!.focuses.length);
	});

	test("a focus that ends after its shot is an error with range [from, shot.end] and the latest start that still fits", () => {
		const d = checkFocusLate(sc);
		expect(d.map((x) => x.message.match(/focus "(\w+)"/)![1])).toEqual(late.map((f) => f.id));
		const x = d[0]!;
		const f = late[0]!;
		expect(x.severity).toBe("error");
		expect(x.code).toBe("camera/focus-late");
		expect(x.message).toContain("s1");
		expect(x.range).toEqual({ start: Math.round(f.from * 100) / 100, end: sc.shots[0]!.end });
		expect(x.measured).toBeCloseTo(f.to, 2);
		expect(x.limit).toBe(sc.shots[0]!.end);
		expect(x.fix!.action).toBe("retime-focus");
		const hold = f.to - f.from;
		expect(x.fix!.params).toEqual({ shot: "s1", focus: f.id, latestAt: Math.round((sc.shots[0]!.end - hold - DEFAULT_LIMITS.settle - 0.05) * 100) / 100 });
	});

	test("a plan whose focuses all end inside the shot passes", () => {
		expect(checkFocusLate(crowded(30))).toEqual([]);
	});

	test("boundary: ending exactly at the shot's end or 0.01 s after passes; 0.02 s after fails", () => {
		const base = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = base.shots[0]!;
		const f = shot.focuses[0]!;
		const withEnd = (end: number): CameraSidecar => withShot(base, { end });
		expect(checkFocusLate(withEnd(f.to))).toEqual([]);
		expect(checkFocusLate(withEnd(f.to - 0.01))).toEqual([]);
		expect(checkFocusLate(withEnd(f.to - 0.02)).length).toBe(1);
	});

	test("a shot without focuses has nothing to be late", () => {
		expect(checkFocusLate(withShot(sc, { focuses: [] }))).toEqual([]);
	});

	test("the fix's latestAt really is early enough: planning the focus there ends inside the shot", () => {
		const f = late[0]!;
		const latestAt = Number(checkFocusLate(sc)[0]!.fix!.params!.latestAt);
		expect(latestAt + DEFAULT_LIMITS.settle + (f.to - f.from)).toBeLessThanOrEqual(sc.shots[0]!.end);
	});

	test("OCR: a late focus is never scanned (checkFocusText ignores it; its frame would be the NEXT scene)", () => {
		const shot = sc.shots[0]!;
		const f = shot.focuses.find((x) => x.from >= shot.end)!;
		expect(f).toBeDefined();
		expect(checkFocusText(sc, vertical, [{ shot, focus: f, boxes: [] }])).toEqual([]);
	});

	test("OCR: scanFocuses extracts no frame for focuses starting at or after the shot's end", async () => {
		const mp4 = join(dir, "late.mp4");
		await run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "color=c=white:s=1080x1920:d=4:r=30", "-c:v", "libx264", "-pix_fmt", "yuv420p", mp4]);
		const seen: string[] = [];
		const scans = await scanFocuses(mp4, sc, async (png) => {
			seen.push(png);
			return { boxes: [] };
		});
		const inShot = sc.shots[0]!.focuses.filter((f) => f.from < sc.shots[0]!.end);
		expect(scans.map((s) => s.focus.id)).toEqual(inShot.map((f) => f.id));
		expect(seen.length).toBe(inShot.length);
		expect(inShot.length).toBeLessThan(sc.shots[0]!.focuses.length);
	}, 30_000);
});


describe("safe-area/caption", () => {
	const cue = (rect: CaptionCue["rect"]): CaptionCue => ({ start: 1, end: 2.5, text: "Hello there", lines: ["Hello there"], rect, fontPx: 64 });
	const sc = (rect: CaptionCue["rect"]) => ({ ...sidecar0, captions: [cue(rect)] });

	test("inside the safe rect passes", () => {
		expect(checkCaptionSafeArea(sc({ x: 100, y: 1200, w: 800, h: 150 }), vertical)).toEqual([]);
	});

	test("in the bottom zone: error naming every platform whose zone it hits, with the safe rect as fix", () => {
		// y 1500..1650: TikTok bottom 25% starts at 1440, Reels 20% at 1536, Shorts 22% at 1498: hits all three (bottom 1650 > each)
		const d = checkCaptionSafeArea(sc({ x: 100, y: 1500, w: 800, h: 150 }), vertical);
		expect(d.length).toBe(1);
		expect(d[0]!.severity).toBe("error");
		expect(d[0]!.code).toBe("safe-area/caption");
		expect(d[0]!.range).toEqual({ start: 1, end: 2.5 });
		for (const p of ["reels", "tiktok", "shorts"]) expect(d[0]!.message).toContain(p);
		expect(d[0]!.fix!.action).toBe("move-out-of-safe-zone");
		expect(d[0]!.fix!.params).toEqual({ x: 65, y: 269, w: 864, h: 1171 });
	});

	test("a caption that only enters the strictest platform's zone names only that platform", () => {
		// bottom edge at 1450: past TikTok's 1440 (25%) only; Reels allows to 1536, Shorts to 1498
		const d = checkCaptionSafeArea(sc({ x: 100, y: 1300, w: 800, h: 150 }), vertical);
		expect(d.length).toBe(1);
		expect(d[0]!.message).toContain("tiktok");
		expect(d[0]!.message).not.toContain("reels");
		expect(d[0]!.message).not.toContain("shorts");
	});

	test("boundary: bottom edge exactly at the strictest zone passes", () => {
		expect(checkCaptionSafeArea(sc({ x: 100, y: 1290, w: 800, h: 150 }), vertical)).toEqual([]);
	});

	test("square profile names the feed platforms", () => {
		const s = { ...sidecar0, frame: { w: 1080, h: 1080 }, captions: [cue({ x: 100, y: 1000, w: 800, h: 70 })] };
		const d = checkCaptionSafeArea(s, square);
		expect(d[0]!.message).toContain("instagram");
	});
});

describe("captions text rules", () => {
	const cap = (start: number, end: number, lines: string[]): CaptionCue => ({ start, end, text: lines.join(" "), lines, rect: { x: 100, y: 1200, w: 800, h: 150 }, fontPx: 64 });
	const sc = (...cues: CaptionCue[]): CameraSidecar => ({ ...sidecar0, captions: cues });
	const codes = (cues: CaptionCue[]) => checkCaptionText(sc(...cues), vertical).map((x) => x.code);

	test("a well-formed track passes", () => {
		expect(codes([cap(0, 2, ["Open the panel"]), cap(2.1, 4.5, ["See every service", "at a glance"])])).toEqual([]);
	});

	test("captions/cps: chars over duration above maxCps (17)", () => {
		const text = "x".repeat(26);
		expect(codes([cap(0, 1.2, [text])])).toContain("captions/cps"); // 26/1.2 = 21.7
		expect(codes([cap(0, 1.6, [text])])).not.toContain("captions/cps"); // 26/1.6 = 16.25
	});

	test("captions/cps boundary: exactly maxCps passes", () => {
		const text = "x".repeat(17);
		expect(codes([cap(0, 1, [text])])).not.toContain("captions/cps");
		expect(codes([cap(0, 0.99, [text])])).toContain("captions/cps");
	});

	test("captions/line-length: a line longer than maxCharsPerLine (27)", () => {
		expect(codes([cap(0, 5, ["x".repeat(27)])])).not.toContain("captions/line-length");
		expect(codes([cap(0, 5, ["x".repeat(28)])])).toContain("captions/line-length");
	});

	test("captions/lines: more than maxLines (2)", () => {
		expect(codes([cap(0, 5, ["a", "b", "c"])])).toContain("captions/lines");
		expect(codes([cap(0, 5, ["a", "b"])])).not.toContain("captions/lines");
	});

	test("captions/duration: shorter than minDuration (0.8)", () => {
		expect(codes([cap(0, 0.79, ["ok"])])).toContain("captions/duration");
		expect(codes([cap(0, 0.8, ["ok"])])).not.toContain("captions/duration");
	});

	test("captions/gap: a gap to the next cue in (0, minGap) is an error; zero gap and a wide gap pass; overlap is an error", () => {
		expect(codes([cap(0, 2, ["a b c"]), cap(2.03, 4, ["d e f"])])).toContain("captions/gap");
		expect(codes([cap(0, 2, ["a b c"]), cap(2, 4, ["d e f"])])).not.toContain("captions/gap");
		expect(codes([cap(0, 2, ["a b c"]), cap(2.1, 4, ["d e f"])])).not.toContain("captions/gap");
		const overlap = checkCaptionText(sc(cap(0, 2, ["a b c"]), cap(1.9, 4, ["d e f"])), vertical).filter((x) => x.code === "captions/gap");
		expect(overlap.length).toBe(1);
		expect(overlap[0]!.severity).toBe("error");
		expect(overlap[0]!.message).toContain("overlap");
	});

	test("every diagnostic is an error with a cut-seconds range and the rechunk fix carrying the profile's numbers", () => {
		const d = checkCaptionText(sc(cap(3, 3.5, ["x".repeat(40), "b", "c"])), vertical);
		expect(d.length).toBeGreaterThanOrEqual(4);
		for (const x of d) {
			expect(x.severity).toBe("error");
			expect(x.range).toEqual({ start: 3, end: 3.5 });
			expect(x.fix).toMatchObject({ action: "rechunk-captions", params: { maxCps: 17, maxCharsPerLine: 27 } });
		}
	});

	test("no captions or a profile without caption rules: nothing", () => {
		expect(checkCaptionText(sidecar0, vertical)).toEqual([]);
		expect(checkCaptionText(sc(cap(0, 0.1, ["x".repeat(99)])), { ...vertical, captions: undefined })).toEqual([]);
	});
});

describe("generic text rules in a camera cut: the viewport's text is the app's, judged only through focuses", () => {
	// Incident: linting the first 9:16 render gave 115 text/too-small errors for the app's own 13 px log lines inside the
	// camera viewport. The camera rules (text/too-small-for-target) judge what the shot asks the viewer to read; the
	// generic rule must keep judging headlines and captions OUTSIDE the viewport.
	const vp = { x: 0, y: 450, w: 1080, h: 790 };
	const box = (x: number, y: number) => ({ text: "x", rect: { x, y, w: 100, h: 12 }, conf: 1 });

	test("boxes fully inside a camera viewport at that time are dropped; the rest are kept", () => {
		const kept = outsideViewports([box(10, 500), box(10, 100), box(10, 1300)], [vp]);
		expect(kept.map((b) => b.rect.y)).toEqual([100, 1300]);
	});

	test("a box that straddles the viewport edge but is mostly OUTSIDE it is authored text (a headline over the picture) and is kept", () => {
		// 12 px tall, 10 of them above the viewport top (y=450): the headline's descenders touching the picture
		expect(outsideViewports([{ text: "x", rect: { x: 10, y: 440, w: 100, h: 12 }, conf: 1 }], [vp])).toHaveLength(1);
	});

	test("a box cut by the viewport edge that is mostly INSIDE it is the picture's own text (a line the camera crops), and is dropped", () => {
		// incident: "atlas" and "GPU terminal" of the take, cut by the viewport's top edge, were judged as authored text
		expect(outsideViewports([{ text: "atlas", rect: { x: 10, y: 440, w: 100, h: 40 }, conf: 1 }], [vp])).toHaveLength(0);
		expect(outsideViewports([{ text: "x", rect: { x: 1000, y: 700, w: 100, h: 20 }, conf: 1 }], [vp])).toHaveLength(0); // cut by the right edge
	});

	test("viewportsAt gives the viewport of the shot on screen at a time, and none for shots without a camera", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = sc.shots[0]!;
		expect(viewportsAt(sc, (shot.start + shot.end) / 2)).toHaveLength(1);
		expect(viewportsAt(sc, shot.end + 1)).toHaveLength(0);
		expect(viewportsAt({ ...sc, shots: [{ ...shot, viewport: undefined }] }, (shot.start + shot.end) / 2)).toHaveLength(0);
	});
});

describe("sampleTimes: the frames the generic rules look at", () => {
	test("without a sidecar: the 25/50/75% stills, as before", () => {
		expect(sampleTimes(undefined, 40)).toEqual([10, 20, 30]);
	});

	test("with a sidecar: one still per shot, at the middle of the shot (a thumbnail of every beat, never mid-fade at the edges)", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const shot = sc.shots[0]!;
		expect(sampleTimes(sc, 40)).toEqual([+((shot.start + shot.end) / 2).toFixed(3)]);
	});
});

describe("frameKindAt: window-edge rules judge frames that HAVE a window", () => {
	// Incident: the 9:16 end card (logo, title, URL, then a caption far below) was reported frame/edge-clipped and
	// window-cropped at 27.5 s: the subject detector joined the title block and the caption into one 'subject' that ran
	// from the top edge to 1397 px. A title card has no window; only shots with a viewport (a recording) do.
	test("a shot with a viewport is a picture; a shot without one is a card", () => {
		const sc = sidecarOf([focusAt("a", 2, 800, 500)]);
		const pic = sc.shots[0]!;
		const card: ShotPlan = { ...pic, id: "end", start: pic.end, end: pic.end + 5, viewport: undefined, keys: [], focuses: [] };
		const both = { ...sc, shots: [pic, card] };
		expect(frameKindAt(both, (pic.start + pic.end) / 2)).toBe("picture");
		expect(frameKindAt(both, card.start + 1)).toBe("card");
	});

	test("no sidecar, or a time outside every shot, is a plain frame (the generic rules apply as before)", () => {
		expect(frameKindAt(undefined, 3)).toBe("frame");
		expect(frameKindAt(sidecarOf([focusAt("a", 2, 800, 500)]), 999)).toBe("frame");
	});
});
