import { createHash } from "node:crypto";
import { StudioError } from "./paths.ts";

export interface RunOpts {
	cwd?: string;
	env?: Record<string, string>;
	/** inherit stdio so progress shows; otherwise output is captured and returned */
	live?: boolean;
	allowFail?: boolean;
}

export async function run(cmd: string[], opts: RunOpts = {}): Promise<{ code: number; out: string; err: string }> {
	const p = Bun.spawn(cmd, {
		cwd: opts.cwd,
		env: { ...process.env, ...opts.env },
		stdout: opts.live ? "inherit" : "pipe",
		stderr: opts.live ? "inherit" : "pipe",
	});
	const [out, err] = opts.live
		? ["", ""]
		: await Promise.all([new Response(p.stdout as ReadableStream).text(), new Response(p.stderr as ReadableStream).text()]);
	const code = await p.exited;
	if (code !== 0 && !opts.allowFail) {
		throw new StudioError(`${cmd[0]} exited ${code}`, (err || out).trim().split("\n").slice(-12).join("\n"));
	}
	return { code, out, err };
}

export async function ffmpeg(args: string[], opts: RunOpts = {}) {
	return run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", ...args], opts);
}

export async function probe(file: string): Promise<{ duration: number; width?: number; height?: number; fps?: number; hasAudio: boolean }> {
	const { out } = await run(["ffprobe", "-v", "error", "-show_entries", "format=duration:stream=codec_type,width,height,r_frame_rate", "-of", "json", file]);
	const j = JSON.parse(out) as { format: { duration?: string }; streams: Array<{ codec_type: string; width?: number; height?: number; r_frame_rate?: string }> };
	const v = j.streams.find((s) => s.codec_type === "video");
	const [n, d] = (v?.r_frame_rate ?? "0/1").split("/").map(Number) as [number, number];
	return {
		duration: Number(j.format.duration ?? 0),
		width: v?.width,
		height: v?.height,
		fps: d ? n / d : undefined,
		hasAudio: j.streams.some((s) => s.codec_type === "audio"),
	};
}

export async function sha256File(file: string): Promise<string> {
	return new Bun.CryptoHasher("sha256").update(await Bun.file(file).arrayBuffer()).digest("hex");
}

export const sha256 = (s: string | Uint8Array) => createHash("sha256").update(s).digest("hex");

/** Integrated loudness (LUFS) and true peak (dBFS) of a file's audio, via ffmpeg ebur128. */
export async function loudness(file: string): Promise<{ lufs: number; peak: number }> {
	const { err } = await run(["ffmpeg", "-nostats", "-hide_banner", "-i", file, "-af", "ebur128=peak=true", "-f", "null", "-"], { allowFail: true });
	const sum = err.slice(err.lastIndexOf("Summary:"));
	const lufs = /I:\s+(-?[\d.]+) LUFS/.exec(sum)?.[1];
	const peak = /Peak:\s+(-?[\d.]+) dBFS/.exec(sum)?.[1];
	if (lufs === undefined) throw new StudioError(`could not measure loudness of ${file}`, err.slice(-400));
	return { lufs: Number(lufs), peak: Number(peak ?? "0") };
}
