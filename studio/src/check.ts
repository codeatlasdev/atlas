// `bun studio check`: the studio's gate. Fast enough to run before every commit that touches studio/.
//   1. tsc --noEmit
//   2. brand tokens match the website (site/src/styles/global.css)
//   3. film data is consistent (unique ids, assets in the manifest, chapters and voice inside the film)
//   4. smoke render: every scene of every cut at 1 s each, through the real compile + HyperFrames check + render
//   5. determinism: one smoke cut rendered twice decodes to identical frames

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { COLOR } from "./brand/tokens.ts";
import { hasAsset } from "./lib/assets.ts";
import { loadFilm, listFilms } from "./lib/films.ts";
import { probe, run } from "./lib/exec.ts";
import { BUILD, REPO, STUDIO, StudioError } from "./lib/paths.ts";
import type { Film } from "./lib/types.ts";
import { renderCut } from "./render.ts";

const problems: string[] = [];
const fail = (msg: string) => problems.push(msg);

async function step<T>(name: string, fn: () => Promise<T>): Promise<T | undefined> {
	const t0 = Date.now();
	try {
		const r = await fn();
		console.log(`✓ ${name} (${((Date.now() - t0) / 1000).toFixed(1)} s)`);
		return r;
	} catch (e) {
		const msg = e instanceof StudioError ? `${e.message}${e.hint ? `\n    ${e.hint.replace(/\n/g, "\n    ")}` : ""}` : String(e);
		console.error(`✗ ${name}\n    ${msg}`);
		fail(name);
		return undefined;
	}
}

function brandSync(): void {
	const css = readFileSync(join(REPO, "site/src/styles/global.css"), "utf8");
	const want: Record<string, string> = { blue: COLOR.blue, "blue-deep": COLOR.blueDeep, paper: COLOR.paper, ink: COLOR.ink, go: COLOR.go, wait: COLOR.wait, fail: COLOR.fail, port: COLOR.port };
	for (const [name, value] of Object.entries(want)) {
		const m = new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`).exec(css);
		if (!m) throw new StudioError(`--${name} not found in site/src/styles/global.css`);
		if (m[1]!.toLowerCase() !== value) throw new StudioError(`brand token ${name} drifted: site ${m[1]}, studio ${value}`, "update studio/src/brand/tokens.ts (the site is the source of truth)");
	}
}

function validate(film: Film): void {
	for (const cut of film.cuts) {
		const ids = new Set<string>();
		let total = 0;
		for (const s of cut.scenes) {
			if (ids.has(s.id)) throw new StudioError(`${film.id}/${cut.out}: duplicate scene id "${s.id}"`);
			ids.add(s.id);
			total += s.duration - (s.transitionIn?.type === "crossfade" ? s.transitionIn.duration : 0);
			if (!hasAsset(`bg:${s.bg ?? "a"}`)) throw new StudioError(`${cut.out}/${s.id}: background bg:${s.bg ?? "a"} is not in the manifest`, "bun studio assets");
			if (s.type === "app" && !hasAsset(s.clip)) throw new StudioError(`${cut.out}/${s.id}: ${s.clip} is not in the manifest`, "bun studio capture app <scene>");
			if (s.type === "terminal" && !hasAsset(s.take)) throw new StudioError(`${cut.out}/${s.id}: ${s.take} is not in the manifest`, "bun studio capture tui <take>");
		}
		for (const c of cut.chapters ?? []) if (c.t < 0 || c.t >= total) throw new StudioError(`${cut.out}: chapter "${c.title}" at ${c.t}s is outside the film (${total.toFixed(2)} s)`);
		const lines = cut.voice?.lines ?? [];
		for (const [i, l] of lines.entries()) {
			if (l.at < 0 || l.at >= total) throw new StudioError(`${cut.out}: line ${l.id} at ${l.at}s is outside the film`);
			if (i > 0 && l.at <= lines[i - 1]!.at) throw new StudioError(`${cut.out}: line ${l.id} starts before ${lines[i - 1]!.id}`);
		}
		if (film.music && !hasAsset(film.music.bed)) throw new StudioError(`${film.id}: music bed ${film.music.bed} is not in the manifest`);
	}
}

/** One second of every scene, silent: proves every scene compiles, lints and renders without paying for the full film. */
async function smoke(film: Film): Promise<void> {
	for (const cut of film.cuts) {
		const smokeCut = { ...cut, voice: undefined, chapters: undefined, overlays: undefined, captions: { vtt: false, burned: false } };
		const r = await renderCut({ ...film, music: undefined }, smokeCut, { draft: true, minimal: true, compile: { clampSeconds: 1 }, out: join(BUILD, "smoke", film.id) });
		const want = cut.scenes.reduce((n, s) => n + Math.min(1, s.duration) - (s.transitionIn?.type === "crossfade" ? s.transitionIn.duration : 0), 0);
		if (Math.abs(r.duration - want) > 0.1) throw new StudioError(`${cut.out}: smoke render is ${r.duration.toFixed(2)} s, expected ${want.toFixed(2)} s`);
	}
}

/** Render the same smoke cut twice and compare the decoded frames (encoder metadata may differ; pixels must not). */
async function determinism(film: Film): Promise<void> {
	const cut = film.cuts[0]!;
	const first = { ...cut, scenes: cut.scenes.slice(0, 2), voice: undefined, chapters: undefined, overlays: undefined, captions: { vtt: false, burned: false } };
	const out = (n: number) => join(BUILD, "smoke", `det${n}`);
	for (const n of [1, 2]) await renderCut({ ...film, music: undefined }, first, { draft: true, minimal: true, compile: { clampSeconds: 1 }, out: out(n) });
	const a = join(out(1), `${cut.out}.mp4`);
	const b = join(out(2), `${cut.out}.mp4`);
	if (Math.abs((await probe(a)).duration - (await probe(b)).duration) > 0.01) throw new StudioError("two renders differ in duration");
	const r = await run(["ffmpeg", "-nostats", "-hide_banner", "-i", a, "-i", b, "-lavfi", "psnr", "-f", "null", "-"], { allowFail: true });
	const avg = /average:(\S+)/.exec(r.err)?.[1];
	if (avg !== "inf") throw new StudioError(`two renders of the same cut decode to different frames (PSNR ${avg})`, "something in a scene reads a clock or random value; see determinism rules in studio/AGENTS.md");
}

export async function main(_argv: string[]): Promise<void> {
	await step("typecheck (tsc --noEmit)", async () => {
		await run([join(STUDIO, "node_modules/.bin/tsc"), "-p", STUDIO]);
	});
	await step("brand tokens match the site", async () => brandSync());
	const films: Film[] = [];
	for (const id of listFilms()) {
		const film = await loadFilm(id);
		films.push(film);
		await step(`film data: ${id}`, async () => validate(film));
	}
	for (const film of films) await step(`smoke render (1 s per scene): ${film.id}`, () => smoke(film));
	if (films[0]) await step("determinism (two renders, identical frames)", () => determinism(films[0]!));
	if (problems.length > 0) {
		console.error(`\n${problems.length} check(s) failed`);
		process.exit(1);
	}
	console.log("\nstudio check passed");
}
