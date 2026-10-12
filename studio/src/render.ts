// `bun studio render <film> [--aspect 16:9|9:16|1:1] [--draft]`: compile, check, render the picture with HyperFrames,
// mix audio, mux, and derive webm / vtt / poster / scrub sprite / chapters into studio/out/<film>/.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { cueEnd, mix } from "./audio/mix.ts";
import { compile, type CompileOpts } from "./compile.ts";
import { ffmpeg, loudness, probe, run, sha256File } from "./lib/exec.ts";
import { BUILD, OUT, STUDIO, StudioError } from "./lib/paths.ts";
import type { AspectId, Chapter, Cut, Film } from "./lib/types.ts";

export interface RenderOpts {
	draft?: boolean;
	/** skip webm/poster/sprite (draft and smoke renders) */
	minimal?: boolean;
	/** fail instead of calling ElevenLabs when a line is not cached */
	offline?: boolean;
	compile?: CompileOpts;
	/** output dir override */
	out?: string;
}

export interface RenderReport {
	film: string;
	cut: string;
	aspect: AspectId;
	buildHash: string;
	duration: number;
	width: number;
	height: number;
	fps: number;
	lufs: number;
	peak: number;
	files: Record<string, { sha256: string; bytes: number }>;
	chapters: Chapter[];
	draft: boolean;
}

/** HyperFrames CLI, pinned in studio/package.json. Telemetry is off: renders in CI and offline must not phone home. */
export function hf(args: string[], cwd: string, live = false) {
	return run([join(STUDIO, "node_modules/.bin/hyperframes"), ...args], { cwd, live, env: { HYPERFRAMES_NO_TELEMETRY: "1", DO_NOT_TRACK: "1" } });
}

export async function renderCut(film: Film, cut: Cut, opts: RenderOpts = {}): Promise<RenderReport> {
	const c = await compile(film, cut, opts.compile);
	const lint = await hf(["check", "--json"], c.dir).catch((e: StudioError) => {
		throw new StudioError(`hyperframes check failed for ${cut.out}`, e.hint);
	});
	void lint;
	const out = opts.out ?? join(OUT, film.id);
	mkdirSync(out, { recursive: true });
	const work = join(BUILD, film.id, `${cut.aspect.replace(":", "x")}-work`);
	mkdirSync(work, { recursive: true });
	const picture = join(work, "picture.mp4");
	// HyperFrames renders a high-quality intermediate (CRF 14); the delivery file is encoded once here at film.crf with a
	// slow preset, so the shipped size is a film parameter and not an accident of the capture's grain.
	const quality = opts.draft ? ["--quality", "draft"] : ["--crf", "14"];
	await hf(["render", "--output", picture, "--fps", String(film.fps), ...quality, "--quiet"], c.dir, false);

	const mixed = await mix(film, cut, c.duration, work, { offline: opts.offline });
	const mp4 = join(out, `${cut.out}.mp4`);
	const video = opts.draft ? ["-c:v", "copy"] : ["-c:v", "libx264", "-crf", String(film.crf), "-preset", "slow", "-pix_fmt", "yuv420p"];
	await ffmpeg(["-i", picture, "-i", mixed.file, "-map", "0:v", "-map", "1:a", ...video, "-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart", "-t", String(c.duration), mp4]);

	const files: RenderReport["files"] = {};
	const add = async (name: string) => {
		const p = join(out, name);
		files[name] = { sha256: await sha256File(p), bytes: Bun.file(p).size };
	};
	await add(`${cut.out}.mp4`);

	if (cut.captions.vtt && cut.voice) {
		writeFileSync(join(out, `${cut.out}.vtt`), vtt(cut, mixed.lineSeconds));
		await add(`${cut.out}.vtt`);
	}
	if (!opts.minimal && !opts.draft) {
		await ffmpeg(["-i", mp4, "-c:v", "libvpx-vp9", "-crf", "33", "-b:v", "0", "-row-mt", "1", "-c:a", "libopus", "-b:a", "128k", join(out, `${cut.out}.webm`)]);
		await add(`${cut.out}.webm`);
		const at = Math.min(cut.poster ?? c.duration / 2, c.duration - 0.1);
		await ffmpeg(["-ss", String(at), "-i", mp4, "-frames:v", "1", "-vf", `scale=${c.width}:-2`, "-q:v", "3", join(out, `${cut.out}-poster.jpg`)]);
		await add(`${cut.out}-poster.jpg`);
		if (cut.chapters) {
			await sprite(mp4, out, cut.out, c.duration);
			await add(`${cut.out}-thumbs.jpg`);
			await add(`${cut.out}-thumbs.vtt`);
		}
	}
	const measured = await probe(mp4);
	const report: RenderReport = {
		film: film.id,
		cut: cut.out,
		aspect: cut.aspect,
		buildHash: c.hash,
		duration: measured.duration,
		width: c.width,
		height: c.height,
		fps: film.fps,
		lufs: (await loudness(mp4)).lufs,
		peak: mixed.peak,
		files,
		chapters: cut.chapters ?? [],
		draft: opts.draft ?? false,
	};
	writeFileSync(join(out, `${cut.out}.report.json`), `${JSON.stringify(report, null, "\t")}\n`);
	return report;
}

const ts = (t: number) => `${String(Math.floor(t / 3600)).padStart(2, "0")}:${String(Math.floor((t % 3600) / 60)).padStart(2, "0")}:${(t % 60).toFixed(3).padStart(6, "0")}`;

/** WebVTT of the narration: cue starts are the line starts, ends are the speech end + 0.4 s, never into the next cue. */
export function vtt(cut: Cut, seconds: Record<string, number>): string {
	const lines = cut.voice?.lines ?? [];
	const cues = lines.map((l, i) => `${i + 1}\n${ts(l.at)} --> ${ts(cueEnd(l.at, seconds[l.id] ?? 3, lines[i + 1]?.at))}\n${l.text}\n`);
	return `WEBVTT\n\n${cues.join("\n")}`;
}

/** Scrub preview sprite (one 160x90 tile every 2 s, 10 per row) + its WebVTT, for the site player. */
async function sprite(mp4: string, out: string, stem: string, duration: number): Promise<void> {
	const step = 2;
	const [tw, th, cols] = [160, 90, 10];
	const n = Math.ceil(duration / step);
	const rows = Math.ceil(n / cols);
	await ffmpeg(["-i", mp4, "-vf", `fps=1/${step},scale=${tw}:${th}:flags=lanczos,tile=${cols}x${rows}`, "-frames:v", "1", "-q:v", "5", join(out, `${stem}-thumbs.jpg`)]);
	const cues: string[] = [];
	for (let i = 0, t = 0; t < duration; i++, t += step) {
		cues.push(`${ts(t)} --> ${ts(Math.min(t + step, duration))}\n${stem}-thumbs.jpg#xywh=${(i % cols) * tw},${Math.floor(i / cols) * th},${tw},${th}\n`);
	}
	writeFileSync(join(out, `${stem}-thumbs.vtt`), `WEBVTT\n\n${cues.join("\n")}`);
}
