// `bun studio lint <film|file|dir> [--profile <id>] [--json] [--ref <file>] [--strict]`
//
// A "video LSP": diagnostics with severity, stable code, pixel rect or time range and a suggested fix, for images
// (posters, README heroes, stills) and rendered videos. Exit code 1 when any error is found (or any warning with
// --strict), so it gates `bun studio check` and `bun studio publish`.

import { existsSync, readdirSync, statSync } from "node:fs";
import { basename, extname, join, relative, resolve } from "node:path";
import { run } from "../lib/exec.ts";
import { listFilms, loadFilm, parseArgs } from "../lib/films.ts";
import { OUT, REPO, StudioError } from "../lib/paths.ts";
import { checkBlack, checkClipping, checkFlashing, checkFrozen, checkLoudness, checkSpec, checkVmaf, isTransition, parseVmaf } from "./checks-video.ts";
import { frameKindAt, lintCameraFile, loadSidecar, outsideViewports, sampleTimes, viewportsAt } from "./camera.ts";
import { analyzeSubject, checkEdgeClipping, checkSafeArea } from "./checks-image.ts";
import { detectBlack, detectClipping, detectFreeze, keyframeTimes, maxKeyframeGap, measureLoudness, probeMedia } from "./media.ts";
import { checkContrast, checkTextSize, ocr, scopeTextToAuthored, withInk } from "./ocr.ts";
import { decode, luminance } from "./pixels.ts";
import { profileFor, profileForAspect } from "./profiles.ts";
import type { Diagnostic, LintReport, Profile, Rect } from "./types.ts";

const IMAGE = /\.(jpe?g|png|webp)$/i;
const VIDEO = /\.(mp4|webm|mov|mkv)$/i;

/** Pick the profile from the file name and shape when none is given. */
export function guessProfile(file: string, w: number, h: number, kind: "image" | "video"): string {
	if (kind === "image") return /^(shot-|loop-.*-poster\.)/.test(basename(file)) ? "site-shot" : "still";
	if (/\.webm$/i.test(file)) return "site-sibling";
	if (basename(file).startsWith("loop-")) return "site-loop";
	const ratio = w / h;
	if (Math.abs(ratio - 9 / 16) < 0.02) return "social-vertical";
	if (Math.abs(ratio - 1) < 0.02) return "social-square";
	if (Math.abs(ratio - 4 / 5) < 0.02) return "social-portrait";
	return basename(file).startsWith("film") ? "site-film" : "social-landscape";
}

/** Mean relative luminance of every frame, from a 64-px-wide decode (cheap, enough for flash detection). */
async function frameLuminance(file: string, fps: number): Promise<number[]> {
	const w = 64;
	const info = await probeMedia(file);
	const h = Math.max(2, Math.round((info.h * w) / info.w / 2) * 2);
	const p = Bun.spawn(["ffmpeg", "-v", "error", "-i", file, "-vf", `fps=${fps},scale=${w}:${h}:flags=area`, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], { stdout: "pipe", stderr: "pipe" });
	const buf = new Uint8Array(await new Response(p.stdout as ReadableStream).arrayBuffer());
	await p.exited;
	const frame = w * h * 3;
	const out: number[] = [];
	for (let o = 0; o + frame <= buf.length; o += frame) {
		let s = 0;
		for (let i = o; i < o + frame; i += 3) s += luminance(buf[i]!, buf[i + 1]!, buf[i + 2]!);
		out.push(s / (w * h));
	}
	return out;
}

export interface LintOpts {
	profile?: string;
	/** a reference video to compare a re-encode against with VMAF */
	ref?: string;
	/** skip the slow checks (OCR/VMAF/flash); `bun studio check` uses it on smoke renders */
	fast?: boolean;
}

/** Rects of the frame that are a camera viewport (a recording under the virtual camera): the app's own picture, framed by the camera rules, not by the generic window/typography rules. */
async function lintImageFile(file: string, profile: Profile, report: LintReport, viewports: Rect[] = [], card = false): Promise<void> {
	const img = await decode(file);
	const d = report.diagnostics;
	// a zoomed recording fills its viewport by design: window-edge rules only apply to frames with no camera
	if (viewports.length === 0 && !card) d.push(...checkEdgeClipping(img, profile));
	const { engine, boxes } = await ocr(file).catch((e: unknown) => {
		report.skipped.push({ check: "text/*, safe-area/*", reason: `OCR unavailable: ${(e as Error).message}` });
		return { engine: "none" as const, boxes: [] };
	});
	if (engine !== "none" && !profile.captured) {
		// legibility/contrast: only what the film authored (the captured app UI is the app's own typography), and only
		// lines a viewer can see (OCR also reads letters into gradients)
		const authored = withInk(img, scopeTextToAuthored(outsideViewports(boxes, viewports), viewports.length ? undefined : analyzeSubject(img).box).authored);
		d.push(...checkTextSize(authored, img.h, profile), ...checkContrast(img, authored, profile), ...checkSafeArea(img, profile, authored));
	}
}

export async function lintFile(file: string, opts: LintOpts = {}): Promise<LintReport> {
	if (!existsSync(file)) throw new StudioError(`${file} does not exist`, "pass a film id, a file or a directory");
	const info = await probeMedia(file);
	const profile = profileFor(opts.profile ?? guessProfile(file, info.w, info.h, info.kind));
	const rel = relative(process.cwd(), file);
	const report: LintReport = {
		version: 1,
		target: { file: rel.startsWith("..") ? file : rel, kind: info.kind, profile: profile.id, width: info.w, height: info.h, duration: info.duration },
		diagnostics: [],
		skipped: [],
		summary: { errors: 0, warnings: 0, infos: 0 },
	};
	const d = report.diagnostics;
	if (info.kind === "image") {
		d.push(...checkSpec(info, profile));
		await lintImageFile(file, profile, report);
	} else {
		const dur = info.duration ?? 0;
		const keys = await keyframeTimes(file);
		d.push(...checkSpec(info, profile, maxKeyframeGap(keys, dur)));
		const black = await detectBlack(file);
		d.push(...checkBlack(black, dur));
		d.push(...checkFrozen(await detectFreeze(file, 4), 8));
		if (info.hasAudio) {
			const loud = await measureLoudness(file);
			if (loud) d.push(...checkLoudness(loud, profile));
			else report.skipped.push({ check: "audio/loudness", reason: "ffmpeg could not measure the audio" });
			d.push(...checkClipping(await detectClipping(file)));
		}
		if (!opts.fast) {
			d.push(...checkFlashing(await frameLuminance(file, 30), 30));
			// OCR a still at the poster-ish middle, a third and two thirds: text that is only briefly on screen is still text
			const tmp = join(process.env.TMPDIR ?? "/tmp", `lint-${process.pid}`);
			await run(["mkdir", "-p", tmp]);
			const sidecar = loadSidecar(file);
			for (const at of sampleTimes(sidecar, dur)) {
				if (isTransition(at, black)) continue; // mid-fade: the title is half transparent, not a defect
				const png = join(tmp, `f${Math.round(at * 1000)}.png`);
				await run(["ffmpeg", "-v", "error", "-y", "-ss", String(at), "-i", file, "-frames:v", "1", png]);
				const sub: LintReport = { ...report, diagnostics: [], skipped: [] };
				await lintImageFile(png, profile, sub, sidecar ? viewportsAt(sidecar, at) : [], frameKindAt(sidecar, at) === "card");
				for (const x of sub.diagnostics) d.push({ ...x, range: { start: +at.toFixed(2), end: +at.toFixed(2) } });
				report.skipped.push(...sub.skipped.filter((s) => !report.skipped.some((r) => r.check === s.check)));
			}
			await run(["rm", "-rf", tmp]);
		}
		if (opts.ref) {
			const { err } = await run(["ffmpeg", "-nostats", "-hide_banner", "-i", file, "-i", opts.ref, "-lavfi", "[0:v]scale=1920:1080:flags=bicubic[a];[1:v]scale=1920:1080:flags=bicubic[b];[a][b]libvmaf=n_subsample=5", "-f", "null", "-"], { allowFail: true });
			const score = parseVmaf(err);
			if (score === undefined) report.skipped.push({ check: "quality/vmaf", reason: "ffmpeg has no libvmaf or the files differ" });
			else d.push(...checkVmaf(score));
		}
		// the camera rules read the sidecar the render wrote next to the cut; a cut without one has no virtual camera
		await lintCameraFile(file, profile, report, opts.fast);
	}
	for (const x of d) report.summary[x.severity === "error" ? "errors" : x.severity === "warning" ? "warnings" : "infos"]++;
	return report;
}

// ── resolution of <film|file|dir> ─────────────────────────────────────────────────────────────────────────────────

function walk(dir: string): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]));
}

/** Files of a film (its rendered cuts and posters from studio/out/<id>) with the profile each should use. */
export async function filmTargets(id: string): Promise<Array<{ file: string; profile: string }>> {
	const film = await loadFilm(id);
	const out = join(OUT, id);
	if (!existsSync(out)) throw new StudioError(`${id} is not rendered`, `bun studio render ${id}`);
	return film.cuts.flatMap((cut) => {
		const profile = profileForAspect(cut.aspect);
		return [`${cut.out}.mp4`, `${cut.out}.webm`, `${cut.out}-poster.jpg`]
			.map((f) => join(out, f))
			.filter((f) => existsSync(f))
			.map((file) => ({ file, profile: IMAGE.test(file) ? "still" : profile }));
	});
}

export async function resolveTargets(arg: string): Promise<Array<{ file: string; profile?: string }>> {
	if (listFilms().includes(arg)) return filmTargets(arg);
	const p = resolve(arg);
	if (!existsSync(p)) throw new StudioError(`"${arg}" is not a film, file or directory`, `films: ${listFilms().join(", ")}`);
	if (statSync(p).isDirectory()) return walk(p).filter((f) => (IMAGE.test(f) || VIDEO.test(f)) && !/-thumbs\.jpg$/.test(f)).sort().map((file) => ({ file }));
	return [{ file: p }];
}

function format(r: LintReport): string {
	const t = r.target;
	const head = `${t.file}  ${t.width}x${t.height}${t.duration ? ` ${t.duration.toFixed(1)}s` : ""}  [${t.profile}]`;
	if (r.diagnostics.length === 0) return `✓ ${head}${r.skipped.length ? `  (skipped: ${r.skipped.map((s) => s.check).join(", ")})` : ""}`;
	const mark = { error: "✗", warning: "!", info: "·" } as const;
	const lines = r.diagnostics.map((x) => {
		const where = x.rect ? ` @ ${x.rect.x},${x.rect.y} ${x.rect.w}x${x.rect.h}` : x.range ? ` @ ${x.range.start.toFixed(2)}s` : "";
		return `  ${mark[x.severity]} ${x.code}${where}: ${x.message}${x.fix ? `\n      fix: ${x.fix.description}` : ""}`;
	});
	return `${r.summary.errors ? "✗" : "!"} ${head}\n${lines.join("\n")}${r.skipped.length ? `\n  skipped: ${r.skipped.map((s) => `${s.check} (${s.reason})`).join("; ")}` : ""}`;
}

export async function lintTargets(targets: Array<{ file: string; profile?: string }>, opts: LintOpts = {}): Promise<LintReport[]> {
	const out: LintReport[] = [];
	for (const t of targets) out.push(await lintFile(t.file, { ...opts, profile: opts.profile ?? t.profile }));
	return out;
}

export const errorCount = (reports: LintReport[]) => reports.reduce((n, r) => n + r.summary.errors, 0);

export async function main(argv: string[]): Promise<void> {
	const { pos, flags } = parseArgs(argv);
	if (!pos[0]) throw new StudioError("lint needs a film id, a file or a directory", "bun studio lint atlas-1-0 | site/public/media/film-poster.jpg | site/public/media");
	const targets = await resolveTargets(pos[0]);
	if (targets.length === 0) throw new StudioError(`nothing to lint in ${pos[0]}`, "images (jpg/png/webp) and videos (mp4/webm/mov/mkv)");
	const reports = await lintTargets(targets, {
		profile: typeof flags.profile === "string" ? flags.profile : undefined,
		ref: typeof flags.ref === "string" ? resolve(flags.ref) : undefined,
		fast: flags.fast === true,
	});
	if (flags.json) console.log(JSON.stringify(reports.length === 1 ? reports[0] : reports, null, "\t"));
	else for (const r of reports) console.log(format(r));
	const errors = errorCount(reports);
	const warnings = reports.reduce((n, r) => n + r.summary.warnings, 0);
	if (!flags.json) console.log(`\n${reports.length} file(s): ${errors} error(s), ${warnings} warning(s)`);
	if (errors > 0 || (flags.strict && warnings > 0)) process.exit(1);
}

export { REPO, extname };
export type { Diagnostic };
