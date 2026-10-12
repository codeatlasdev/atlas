// `bun studio preview <film> [--aspect 16:9] [--port 3002]`: compile the cut, open HyperFrames Studio on it, and rebuild
// whenever film data or scene code changes (HyperFrames reloads the preview when the project files change).

import { watch } from "node:fs";
import { join } from "node:path";
import { compile } from "./compile.ts";
import { loadFilm, parseArgs, pickCuts } from "./lib/films.ts";
import { FILMS, STUDIO } from "./lib/paths.ts";
import type { Film } from "./lib/types.ts";
import { hf } from "./render.ts";

export async function main(argv: string[]): Promise<void> {
	const { pos, flags } = parseArgs(argv);
	const id = pos[0] ?? "";
	const aspect = typeof flags.aspect === "string" ? flags.aspect : "16:9";
	const port = typeof flags.port === "string" ? flags.port : "3002";
	let film = await loadFilm(id);
	const cut = () => pickCuts(film, aspect)[0]!;
	const c = await compile(film, cut());
	console.log(`compiled ${film.id} ${aspect}: ${c.duration.toFixed(2)} s -> ${c.dir}`);

	let timer: ReturnType<typeof setTimeout> | undefined;
	let building = false;
	const rebuild = () => {
		clearTimeout(timer);
		timer = setTimeout(async () => {
			if (building) return;
			building = true;
			try {
				// bust the module cache: films and scene builders are re-read from disk on every rebuild
				film = (await loadFreshFilm(id)) ?? film;
				const r = await compile(film, cut());
				console.log(`rebuilt ${r.duration.toFixed(2)} s (${r.hash})`);
			} catch (e) {
				console.error(`✗ rebuild failed: ${e instanceof Error ? e.message : String(e)}`);
			} finally {
				building = false;
			}
		}, 250);
	};
	for (const dir of [join(FILMS, id), join(STUDIO, "src/scenes"), join(STUDIO, "src/brand")]) watch(dir, { recursive: true }, rebuild);
	console.log(`watching films/${id}, src/scenes, src/brand; Ctrl-C to stop`);
	await hf(["preview", "--port", port, "--foreground", flags.open === true ? "--open" : "--no-open"], c.dir, true);
}

/** Re-import film.ts with a query string so Bun re-evaluates it instead of returning the cached module. */
async function loadFreshFilm(id: string): Promise<Film | undefined> {
	const url = `${join(FILMS, id, "film.ts")}?t=${Date.now()}`;
	// the specifier carries a cache-busting query computed at run time, so it cannot be a static import
	const mod: { default?: Film } = await import(url);
	return mod.default;
}
