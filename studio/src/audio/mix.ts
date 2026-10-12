// Audio mix: narration lines placed at their `at`, compressed and normalised, a music bed sidechain-ducked by the
// voice, then a measured loudness correction. The final integrated loudness is MEASURED (ffmpeg ebur128) and the
// render fails when it is outside the film's tolerance: the number in the report is a measurement, not a promise.

import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { assetPath } from "../lib/assets.ts";
import { ffmpeg, loudness, probe } from "../lib/exec.ts";
import { StudioError } from "../lib/paths.ts";
import type { Cut, Film } from "../lib/types.ts";
import { lineSeconds, lineWav } from "./voice.ts";

export interface Mixed {
	/** the mixed audio, 48 kHz stereo wav, exactly `duration` long */
	file: string;
	lufs: number;
	peak: number;
	/** seconds of audio of each narration line, for captions */
	lineSeconds: Record<string, number>;
}

/** Caption cue end: the line's speech plus a 0.4 s tail (reads better than cutting on the last phoneme), never into the next cue. */
export function cueEnd(at: number, seconds: number, nextAt: number | undefined): number {
	const end = at + seconds + 0.4;
	return nextAt === undefined ? end : Math.min(end, nextAt - 0.01);
}

/** Build the voice track: every line delayed to its start, summed, compressed, normalised to the voice target. */
async function voiceTrack(cut: Cut, duration: number, work: string, offline: boolean): Promise<{ file: string; seconds: Record<string, number> }> {
	const v = cut.voice!;
	const seconds: Record<string, number> = {};
	const inputs: string[] = [];
	const filters: string[] = [];
	for (const [i, l] of v.lines.entries()) {
		inputs.push("-i", await lineWav(v, l, { offline }));
		seconds[l.id] = await lineSeconds(v, l, { offline });
		filters.push(`[${i}:a]adelay=${Math.round(l.at * 1000)}|${Math.round(l.at * 1000)}[l${i}]`);
	}
	const mix = `${v.lines.map((_, i) => `[l${i}]`).join("")}amix=inputs=${v.lines.length}:normalize=0:dropout_transition=0,apad=whole_dur=${duration},atrim=0:${duration},acompressor=threshold=-18dB:ratio=3:attack=5:release=120,loudnorm=I=${v.lufs}:TP=-1.5:LRA=7[v]`;
	const file = join(work, "voice.wav");
	await ffmpeg([...inputs, "-filter_complex", `${filters.join(";")};${mix}`, "-map", "[v]", "-ar", "48000", "-ac", "2", file]);
	return { file, seconds };
}

/** Mix voice + bed for a cut. Without voice or music the result is silence of the right length (never absent). */
export async function mix(film: Film, cut: Cut, duration: number, work: string, opts: { offline?: boolean } = {}): Promise<Mixed> {
	mkdirSync(work, { recursive: true });
	const out = join(work, "mix.wav");
	const music = film.music;
	if (!cut.voice && !music) {
		await ffmpeg(["-f", "lavfi", "-i", `anullsrc=r=48000:cl=stereo`, "-t", String(duration), out]);
		return { file: out, lufs: -70, peak: -70, lineSeconds: {} };
	}
	const voice = cut.voice ? await voiceTrack(cut, duration, work, opts.offline ?? false) : undefined;
	const raw = join(work, "mix-raw.wav");
	const bed = music ? assetPath(music.bed) : undefined;
	if (voice && bed && music) {
		const d = music.duck;
		await ffmpeg([
			"-i", bed, "-i", voice.file,
			"-filter_complex",
			`[1:a]asplit=2[vox][sc];` +
				`[0:a]atrim=0:${duration},asetpts=PTS-STARTPTS,afade=t=in:d=${music.fadeIn},afade=t=out:st=${Math.max(0, duration - music.fadeOut)}:d=${music.fadeOut},loudnorm=I=${music.bedLufs}:TP=-1.5:LRA=7[bed];` +
				`[bed][sc]sidechaincompress=threshold=${d.threshold}:ratio=${d.ratio}:attack=${d.attack}:release=${d.release}:makeup=1[duck];` +
				`[duck][vox]amix=inputs=2:normalize=0,alimiter=limit=0.95,atrim=0:${duration}[a]`,
			"-map", "[a]", "-ar", "48000", "-ac", "2", raw,
		]);
	} else if (voice) {
		await ffmpeg(["-i", voice.file, "-af", "alimiter=limit=0.95", "-ar", "48000", "-ac", "2", raw]);
	} else if (bed && music) {
		await ffmpeg(["-i", bed, "-af", `atrim=0:${duration},asetpts=PTS-STARTPTS,afade=t=in:d=${music.fadeIn},afade=t=out:st=${Math.max(0, duration - music.fadeOut)}:d=${music.fadeOut},loudnorm=I=${film.loudness.target}:TP=-1.5:LRA=7`, "-ar", "48000", "-ac", "2", raw]);
	}
	const first = await loudness(raw);
	// one measured linear correction toward the target, then the limiter keeps peaks under -1 dBFS
	const gain = film.loudness.target - first.lufs;
	await ffmpeg(["-i", raw, "-af", `volume=${gain.toFixed(2)}dB,alimiter=limit=0.89:level=false,atrim=0:${duration}`, "-ar", "48000", "-ac", "2", out]);
	const final = await loudness(out);
	if (Math.abs(final.lufs - film.loudness.target) > film.loudness.tolerance) {
		throw new StudioError(
			`${cut.out}: integrated loudness ${final.lufs.toFixed(1)} LUFS is outside ${film.loudness.target} ±${film.loudness.tolerance}`,
			`bed level (music.bedLufs ${music?.bedLufs}) or ducking may be fighting the voice; adjust film.ts and re-render`
		);
	}
	const len = (await probe(out)).duration;
	if (Math.abs(len - duration) > 0.1) throw new StudioError(`${cut.out}: mixed audio is ${len.toFixed(2)} s, expected ${duration.toFixed(2)} s`);
	return { file: out, lufs: final.lufs, peak: final.peak, lineSeconds: voice?.seconds ?? {} };
}
