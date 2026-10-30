// bun studio sounds [generate|pick|list] — builds the Atlas earcons with the ElevenLabs Sound Effects API.
//
//   generate [id…] [--variants N] [--force]   request candidates (cached by prompt hash in .cache/sounds/raw)
//   pick [id…]                                trim, measure, gate by spec, normalise, encode the best per category to
//                                             apps/desktop/static/sounds, write the audition sheet + PROVENANCE.md
//   list                                      what each category asks for and what is on disk
//
// The key is read at run time only (ELEVENLABS_API_KEY, see lib/paths.ts) and is never printed or written.

import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ffmpeg } from "../lib/exec.ts";
import { provenanceRow, readme, TARGET, type Shipped } from "./provenance.ts";
import { CACHE, REPO, secret, StudioError } from "../lib/paths.ts";
import { parseArgs } from "../lib/films.ts";
import { alignDownbeat, analyze, findDownbeat, gainFor, leadingSilence, trimDecay } from "./dsp.ts";
import { capFade, decayDb, EARCONS, EARCON_LIST, isCurrentTake, pickBest, promptFor, passes, takeName, type EarconId, type Stats } from "./spec.ts";

const RAW = join(CACHE, "sounds/raw");
const WORK = join(CACHE, "sounds/work");
const AUDITION = join(CACHE, "sounds/wav");
const OUT = join(REPO, "apps/desktop/static/sounds");
const SR = 48_000;
/** Target feel: RMS of the trimmed earcon, with the peak held under -3 dBFS. Quiet by design; the app scales further. */
/** The model and endpoint the files come from, recorded in PROVENANCE.md. */
const MAX_BYTES = 20 * 1024;

const ids = (pos: string[]): EarconId[] => {
	const all = Object.keys(EARCONS) as EarconId[];
	const chosen = pos.length ? pos : all;
	for (const c of chosen) if (!all.includes(c as EarconId)) throw new StudioError(`unknown earcon "${c}"`, `known: ${all.join(", ")}`);
	return chosen as EarconId[];
};

async function generate(id: EarconId, variants: number, force: boolean): Promise<void> {
	mkdirSync(RAW, { recursive: true });
	for (let v = 0; v < variants; v++) {
		const file = join(RAW, takeName(id, v));
		if (existsSync(file) && !force) continue;
		const e = EARCONS[id];
		const res = await fetch("https://api.elevenlabs.io/v1/sound-generation?output_format=mp3_44100_128", {
			method: "POST",
			headers: { "Content-Type": "application/json", "xi-api-key": secret("ELEVENLABS_API_KEY") },
			// `prompt_influence` high: we want the written gesture followed, not the model's creativity.
			body: JSON.stringify({ text: promptFor(e), duration_seconds: e.requestSeconds, prompt_influence: 0.7 }),
		});
		if (!res.ok) throw new StudioError(`ElevenLabs ${res.status} generating "${id}" #${v}`, (await res.text()).slice(0, 300));
		await Bun.write(file, await res.arrayBuffer());
		console.log(`  ${id} #${v}  ${(statSync(file).size / 1024).toFixed(1)} KB`);
	}
}

/** Decode any file to 48 kHz mono float samples. */
async function decode(file: string): Promise<Float32Array> {
	const p = Bun.spawn(["ffmpeg", "-hide_banner", "-loglevel", "error", "-i", file, "-ac", "1", "-ar", String(SR), "-f", "f32le", "-"], { stdout: "pipe" });
	const buf = await new Response(p.stdout as ReadableStream).arrayBuffer();
	if ((await p.exited) !== 0) throw new StudioError(`ffmpeg could not decode ${file}`);
	return new Float32Array(buf);
}

/** Drop the lead-in silence (keeping 3 ms before the attack) and the padded tail (where the note has decayed DECAY_DB). */
function trim(x: Float32Array, db: number): { y: Float32Array; head: number } {
	const gate = 10 ** (-45 / 20);
	const first = x.findIndex((v) => Math.abs(v) >= gate);
	if (first < 0) return { y: new Float32Array(0), head: x.length / SR };
	const start = Math.max(0, first - Math.round(0.003 * SR));
	return { y: trimDecay(x.slice(start), SR, db), head: leadingSilence(x, SR) };
}

interface Candidate {
	id: string;
	file: string;
	stats: Stats;
	samples: Float32Array;
}

async function measure(id: EarconId): Promise<Candidate[]> {
	const out: Candidate[] = [];
	for (const f of readdirSync(RAW).filter((n) => isCurrentTake(id, n)).sort()) {
		const trimmed = trim(await decode(join(RAW, f)), decayDb(EARCON_LIST[id]));
		const { head } = trimmed;
		// An earcon with a `downbeatSeconds` is cut to a picture: shift it so its strike lands there.
		const at = EARCON_LIST[id].downbeatSeconds;
		const y = at !== undefined && findDownbeat(trimmed.y, SR) !== null ? alignDownbeat(trimmed.y, SR, at) : trimmed.y;
		if (at !== undefined && findDownbeat(trimmed.y, SR) === null) continue;
		const a = analyze(y, SR);
		const cap = Math.round(EARCONS[id].maxSeconds * SR);
		out.push({
			id: f.replace(/\.mp3$/, ""),
			file: join(RAW, f),
			samples: y,
			stats: { duration: a.duration, peakDb: a.peakDb, rmsDb: a.rmsDb, centroidHz: a.centroidHz, clipped: a.clipped, silentHead: head, ...(y.length > cap ? { levelAtCapDb: levelBefore(y, cap) } : {}) },
		});
	}
	return out;
}

/** Level (dB re the take's peak) of the 5 ms just before sample `at`. */
function levelBefore(y: Float32Array, at: number): number {
	let peak = 0;
	for (const v of y) peak = Math.max(peak, Math.abs(v));
	let m = 0;
	for (let i = Math.max(0, at - Math.round(0.005 * SR)); i < at; i++) m = Math.max(m, Math.abs(y[i]!));
	return 20 * Math.log10(Math.max(m, 1e-9) / Math.max(peak, 1e-9));
}

const f32 = (x: Float32Array) => Buffer.from(x.buffer, x.byteOffset, x.byteLength);

/** Write the finished earcon: gain-matched, 5 ms fades, mono 48 kHz, shipped as .m4a (AAC); a .wav copy for auditioning stays in the gitignored cache. */
async function render(id: EarconId, c: Candidate): Promise<{ m4a: number; wav: string; rmsDb: number; peakDb: number }> {
	mkdirSync(WORK, { recursive: true });
	mkdirSync(AUDITION, { recursive: true });
	mkdirSync(OUT, { recursive: true });
	// Over its cap: end the note at the cap with a short fade (the picker only accepts takes that are quiet there).
	const cap = Math.round(EARCONS[id].maxSeconds * SR);
	const body = c.samples.length > cap ? c.samples.slice(0, cap) : c.samples;
	const trimmed = analyze(body, SR);
	const g = gainFor({ rmsDb: trimmed.rmsDb, peakDb: trimmed.peakDb }, TARGET);
	const y = Float32Array.from(body, (v) => v * g);
	const fadeIn = Math.round(0.005 * SR);
	const fadeOut = Math.round((c.samples.length > cap ? capFade(EARCONS[id].maxSeconds) : 0.005) * SR);
	for (let i = 0; i < fadeIn && i < y.length; i++) y[i]! *= i / fadeIn;
	for (let i = 0; i < fadeOut && i < y.length; i++) y[y.length - 1 - i]! *= i / fadeOut;
	const raw = join(WORK, `${id}.f32`);
	await Bun.write(raw, f32(y));
	const input = ["-f", "f32le", "-ar", String(SR), "-ac", "1", "-i", raw];
	const wav = join(AUDITION, `${id}.wav`);
	await ffmpeg([...input, "-c:a", "pcm_s16le", wav]);
	await ffmpeg([...input, "-c:a", "aac", "-b:a", "48k", "-movflags", "+faststart", join(OUT, `${id}.m4a`)]);
	rmSync(raw);
	const after = analyze(y, SR);
	return { m4a: statSync(join(OUT, `${id}.m4a`)).size, wav, rmsDb: after.rmsDb, peakDb: after.peakDb };
}

async function wavePng(id: EarconId, wav: string): Promise<void> {
	const dir = join(CACHE, "sounds/sheet");
	mkdirSync(dir, { recursive: true });
	await ffmpeg(["-i", wav, "-filter_complex", "showwavespic=s=640x120:colors=#7aa2ff", "-frames:v", "1", join(dir, `${id}.png`)]);
}

async function pick(chosen: EarconId[]): Promise<void> {
	const failures: string[] = [];
	for (const id of chosen) {
		const cands = await measure(id);
		if (!cands.length) {
			failures.push(`${id}: no takes (run: bun studio sounds generate ${id})`);
			continue;
		}
		const best = pickBest(EARCONS[id], cands);
		if (!best) {
			const why = cands
				.map((c) => {
					const v = passes(EARCONS[id], c.stats);
					return `${c.id.split(".")[1]}:${v.ok ? "ok" : v.why}`;
				})
				.join(" ");
			failures.push(`${id}: no candidate meets the spec (${why}); generate more with --variants or edit the gesture in spec.ts`);
			continue;
		}
		const r = await render(id, best);
		const finished = Math.min(best.stats.duration, EARCONS[id].maxSeconds);
		await wavePng(id, r.wav);
		if (r.m4a > MAX_BYTES) failures.push(`${id}: ${r.m4a} B exceeds the ${MAX_BYTES} B budget`);
		const take = best.id.split(".")[1];
		const ms = Math.round(finished * 1000);
		console.log(`  ${id.padEnd(16)} take ${take}  ${String(ms).padStart(4)} ms  rms ${r.rmsDb.toFixed(1)} dBFS  peak ${r.peakDb.toFixed(1)} dBFS  centroid ${Math.round(best.stats.centroidHz)} Hz  m4a ${r.m4a} B`);
		recordShipped(id, { ms, rmsDb: r.rmsDb, peakDb: r.peakDb, centroidHz: best.stats.centroidHz, bytes: r.m4a, take: Number(take), key: best.id.split(".")[2]! });
	}
	if (failures.length) throw new StudioError("some earcons could not be finished", failures.join("\n"));
}

/** Record what was shipped for `id` and rebuild the README from every shipped earcon. */
function recordShipped(id: EarconId, shipped: Shipped): void {
	const file = join(OUT, "provenance.json");
	const all: Record<string, Shipped> = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
	all[id] = shipped;
	const sorted = Object.fromEntries(Object.entries(all).sort(([a], [b]) => a.localeCompare(b)));
	writeFileSync(file, `${JSON.stringify(sorted, null, "\t")}\n`);
	// Only earcons that still have a file on disk are listed, in the spec's order.
	const rows = (Object.keys(EARCONS) as EarconId[])
		.filter((e) => all[e] && existsSync(join(OUT, `${e}.m4a`)))
		.map((e) => provenanceRow(e, all[e]!));
	writeFileSync(join(OUT, "README.md"), readme(rows, new Date().toISOString().slice(0, 10)));
}

function list(): void {
	for (const id of Object.keys(EARCONS) as EarconId[]) {
		const e = EARCONS[id];
		const have = existsSync(join(OUT, `${id}.m4a`)) ? `${statSync(join(OUT, `${id}.m4a`)).size} B` : "-";
		console.log(`${id.padEnd(16)} ${e.kind.padEnd(9)} <=${Math.round(e.maxSeconds * 1000)} ms  on disk: ${have}\n    ${e.meaning}\n    ${promptFor(e)}`);
	}
}

export async function main(argv: string[]): Promise<void> {
	const [sub = "list", ...rest] = argv;
	const { pos, flags } = parseArgs(rest);
	if (sub === "list") return list();
	if (sub === "generate") {
		const variants = flags.variants === undefined || flags.variants === true ? 4 : Number(flags.variants);
		if (!Number.isInteger(variants) || variants < 1 || variants > 12) throw new StudioError("--variants must be 1..12");
		for (const id of ids(pos)) {
			console.log(id);
			await generate(id, variants, flags.force === true);
		}
		return;
	}
	if (sub === "pick") return pick(ids(pos));
	throw new StudioError(`unknown sounds command "${sub}"`, "bun studio sounds list|generate|pick");
}
