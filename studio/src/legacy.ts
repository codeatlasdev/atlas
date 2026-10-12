// One-time bridge from the ad-hoc 1.0 pipeline: registers the raw window recordings, the vhs take, the cobalt stills,
// the music bed and the narration mp3s of the 1.0 film into the manifest, so the film rebuilds without any capture or
// API call. Narration is registered under the same hash `voice` would compute, so a later `voice` run reuses it.

import { existsSync } from "node:fs";
import { join } from "node:path";
import film from "../films/atlas-1-0/film.ts";
import { registerAsset } from "./lib/assets.ts";
import { REPO, StudioError } from "./lib/paths.ts";
import { voiceKey } from "./audio/voice.ts";

const LEGACY = "imported:legacy-1.0";

/** Narration files of the 1.0 film by line id: n01..n12 (16:9) and v1..v6 (vertical). */
const voiceFile = (dir: string, id: string) => join(dir, id.startsWith("n") ? `narration3-${id}.mp3` : `vn-${id}.mp3`);

export async function importLegacy(work: string): Promise<string[]> {
	const done: string[] = [];
	const put = async (key: string, kind: Parameters<typeof registerAsset>[1], file: string, meta?: Record<string, unknown>) => {
		if (!existsSync(file)) throw new StudioError(`missing ${file}`, `point --from at the 1.0 working dir ($FILM_WORK, default /tmp/atlas-film/work)`);
		await registerAsset(key, kind, file, LEGACY, meta);
		done.push(key);
	};
	for (const n of ["spaces", "terminal", "panel", "logs", "agents", "mcp"]) await put(`rec:${n}`, "recording", join(work, "rec", `${n}.mov`));
	await put("take:tui", "take", join(REPO, "demo/.out/tui.mp4"));
	for (const b of ["a", "b", "c", "v"]) {
		await put(`bg:${b}`, "background", join(work, "film2", `cobalt_${b}.jpg`), { generator: "Magnific FLUX.2 pro", seed: { a: 21, b: 22, c: 23, v: 24 }[b] });
	}
	await put("music:bed", "music", join(work, "film", "bed.mp3"), { generator: "ElevenLabs Music music_v1" });
	for (const cut of film.cuts) {
		if (!cut.voice) continue;
		for (const l of cut.voice.lines) await put(voiceKey(cut.voice, l), "voice", voiceFile(join(work, "vo"), l.id), { line: l.id, text: l.text, voice: cut.voice.id, model: cut.voice.model });
	}
	return done;
}
