// Source recordings are HEVC at 2x window size; the browser decodes small H.264 proxies instead. A proxy bakes in what
// is fixed per scene (crop, scale, fps, trim, dense cut, speed) and is cached by a hash of its inputs, so editing a
// title never re-encodes footage and re-renders are fast and reproducible.

import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { assetPath } from "./lib/assets.ts";
import { ffmpeg, probe, sha256, sha256File } from "./lib/exec.ts";
import { CACHE } from "./lib/paths.ts";
import type { AppClipScene, TerminalScene } from "./lib/types.ts";

export interface Proxy {
	/** absolute path of the proxy file */
	path: string;
	/** project-relative file name it will be copied to */
	file: string;
	/** width / height of the proxy */
	aspect: number;
	seconds: number;
}

const X264 = ["-c:v", "libx264", "-crf", "14", "-preset", "medium", "-pix_fmt", "yuv420p", "-an", "-movflags", "+faststart"];

async function cached(key: unknown, build: (out: string) => Promise<void>): Promise<string> {
	const dir = join(CACHE, "proxy");
	mkdirSync(dir, { recursive: true });
	const out = join(dir, `${sha256(JSON.stringify(key)).slice(0, 16)}.mp4`);
	if (!existsSync(out)) {
		await build(`${out}.tmp.mp4`);
		await Bun.write(out, Bun.file(`${out}.tmp.mp4`));
		await Bun.file(`${out}.tmp.mp4`).delete();
	}
	return out;
}

/** App recording: seek, crop, scale to the box width (lanczos), 30 fps, for the scene's duration. */
export async function prepareApp(s: AppClipScene, boxW: number, h: number, fps: number): Promise<Proxy> {
	const src = assetPath(s.clip);
	const key = { v: 1, src: await sha256File(src), from: s.from, dur: s.duration, crop: s.crop, w: boxW, h, fps };
	const path = await cached(key, (out) =>
		ffmpeg(["-ss", String(s.from), "-t", String(s.duration), "-i", src, "-vf", `crop=${s.crop.w}:${s.crop.h}:${s.crop.x}:${s.crop.y},scale=${boxW}:${h}:flags=lanczos,fps=${fps},format=yuv420p`, ...X264, "-t", String(s.duration), out]).then(() => undefined)
	);
	return { path, file: `proxy-${s.id}.mp4`, aspect: boxW / h, seconds: s.duration };
}

/** vhs take: trim each segment, join, change speed, scale to the width, 30 fps. */
export async function prepareTerminal(s: TerminalScene, fps: number): Promise<Proxy> {
	const src = assetPath(s.take);
	const speed = s.speed ?? 1;
	const key = { v: 1, src: await sha256File(src), seg: s.segments, speed, w: s.width, dur: s.duration, fps };
	const info = await probe(src);
	const aspect = (info.width ?? 1600) / (info.height ?? 1000);
	const parts = s.segments.map(([a, b], i) => `[0:v]trim=${a}:${b},setpts=PTS-STARTPTS[s${i}]`);
	const labels = s.segments.map((_, i) => `[s${i}]`).join("");
	const filter = `${parts.join(";")};${labels}concat=n=${s.segments.length}:v=1:a=0,setpts=PTS/${speed},scale=${s.width}:-2:flags=lanczos,fps=${fps},format=yuv420p[v]`;
	const path = await cached(key, (out) => ffmpeg(["-i", src, "-filter_complex", filter, "-map", "[v]", ...X264, "-t", String(s.duration), out]).then(() => undefined));
	const out = await probe(path);
	return { path, file: `proxy-${s.id}.mp4`, aspect: (out.width ?? 1) / (out.height ?? 1) || aspect, seconds: s.duration };
}
