// Narration: ElevenLabs text-to-speech, cached by a hash of everything that changes the audio (voice, model, tag,
// settings, text). Re-running `voice` after editing one line only synthesizes that line.

import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { assetPath, hasAsset, registerAsset } from "../lib/assets.ts";
import { ffmpeg, probe, sha256 } from "../lib/exec.ts";
import { CACHE, secret, StudioError } from "../lib/paths.ts";
import type { NarrationLine, VoiceSpec } from "../lib/types.ts";

export interface ElevenModel {
	model_id: string;
	name: string;
	can_do_text_to_speech?: boolean;
}

/** The text actually sent: delivery tag, a space, the line. */
export const spokenText = (v: VoiceSpec, l: NarrationLine) => `${v.tag} ${l.text}`;

/**
 * Cache key of one line's audio. Everything that changes the sound is in it, plus the line id: speech synthesis is not
 * deterministic, so each line owns its take (the 1.0 horizontal and vertical cuts both say "Atlas. Your whole dev
 * stack, in one file." with two different generations) and editing one line never disturbs another.
 */
export function voiceKey(v: VoiceSpec, l: NarrationLine): string {
	const h = sha256(JSON.stringify({ id: l.id, voice: v.id, model: v.model, stability: v.stability, similarity: v.similarity, text: spokenText(v, l) }));
	return `voice:${h.slice(0, 20)}`;
}

/** Path of a line's audio, synthesizing it when the cache misses. */
export async function lineAudio(v: VoiceSpec, l: NarrationLine, opts: { offline?: boolean } = {}): Promise<string> {
	const key = voiceKey(v, l);
	if (hasAsset(key)) return assetPath(key);
	if (opts.offline) {
		throw new StudioError(`no cached audio for line "${l.id}" (${key})`, `run: bun studio voice <film>   (needs ELEVENLABS_API_KEY)`);
	}
	const res = await fetch(`https://api.elevenlabs.io/v1/text-to-speech/${v.id}?output_format=mp3_44100_192`, {
		method: "POST",
		headers: { "Content-Type": "application/json", "xi-api-key": secret("ELEVENLABS_API_KEY") },
		body: JSON.stringify({ model_id: v.model, text: spokenText(v, l), voice_settings: { similarity_boost: v.similarity, stability: v.stability } }),
	});
	if (!res.ok) throw new StudioError(`ElevenLabs ${res.status} for line "${l.id}"`, (await res.text()).slice(0, 300));
	const dir = join(CACHE, "voice");
	mkdirSync(dir, { recursive: true });
	const file = join(dir, `${key.slice(6)}.mp3`);
	await Bun.write(file, await res.arrayBuffer());
	const meta = { voice: v.id, model: v.model, line: l.id, text: l.text };
	await registerAsset(key, "voice", file, "bun studio voice", meta);
	return file;
}

/**
 * Seconds of actual speech in a line: the audio with its trailing silence trimmed (-40 dB). The mp3 container carries
 * 0.1-0.4 s of padding after the last word, which would put caption cues past the voice. Measured once per take and
 * cached beside it.
 */
export async function lineSeconds(v: VoiceSpec, l: NarrationLine, opts: { offline?: boolean } = {}): Promise<number> {
	const side = (await lineAudio(v, l, opts)).replace(/\.mp3$/, ".speech.txt");
	if (existsSync(side)) return Number(await Bun.file(side).text());
	const trimmed = `${side}.wav`;
	await ffmpeg(["-i", await lineAudio(v, l, opts), "-af", "areverse,silenceremove=start_periods=1:start_threshold=-40dB,areverse", trimmed]);
	const seconds = (await probe(trimmed)).duration;
	await Bun.write(side, seconds.toFixed(3));
	await Bun.file(trimmed).delete();
	return seconds;
}

/** Models of the account that can do TTS, newest first as the API lists them. */
export async function listModels(): Promise<ElevenModel[]> {
	const res = await fetch("https://api.elevenlabs.io/v1/models", { headers: { "xi-api-key": secret("ELEVENLABS_API_KEY") } });
	if (!res.ok) throw new StudioError(`ElevenLabs ${res.status} listing models`, (await res.text()).slice(0, 300));
	return ((await res.json()) as ElevenModel[]).filter((m) => m.can_do_text_to_speech);
}

/** Decode one line to a 48 kHz stereo wav next to the mp3 (used by the mixer). */
export async function lineWav(v: VoiceSpec, l: NarrationLine, opts: { offline?: boolean } = {}): Promise<string> {
	const mp3 = await lineAudio(v, l, opts);
	const wav = mp3.replace(/\.mp3$/, ".wav");
	if (!existsSync(wav)) await ffmpeg(["-i", mp3, "-ar", "48000", "-ac", "2", wav]);
	return wav;
}
