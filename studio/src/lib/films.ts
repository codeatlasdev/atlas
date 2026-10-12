import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { FILMS, StudioError } from "./paths.ts";
import { ASPECTS, type AspectId, type Cut, type Film } from "./types.ts";

export const listFilms = () => readdirSync(FILMS).filter((d) => existsSync(join(FILMS, d, "film.ts")));

export interface Parsed {
	pos: string[];
	flags: Record<string, string | true>;
}

export async function loadFilm(id: string): Promise<Film> {
	if (!listFilms().includes(id)) throw new StudioError(`unknown film "${id}"`, `films: ${listFilms().join(", ") || "(none)"}; add studio/films/<id>/film.ts`);
	// the film id is chosen at run time from the studio/films directory, so a static import cannot name it
	const mod: { default: Film } = await import(join(FILMS, id, "film.ts"));
	return mod.default;
}

export function parseArgs(argv: string[]): Parsed {
	const pos: string[] = [];
	const flags: Record<string, string | true> = {};
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i]!;
		if (!a.startsWith("--")) {
			pos.push(a);
			continue;
		}
		const next = argv[i + 1];
		if (next !== undefined && !next.startsWith("--")) {
			flags[a.slice(2)] = next;
			i++;
		} else flags[a.slice(2)] = true;
	}
	return { pos, flags };
}

/** Cuts to act on: all of them, or the ones matching `--aspect`. */
export function pickCuts(film: Film, aspect: string | true | undefined): Cut[] {
	if (aspect === undefined) return film.cuts;
	if (typeof aspect !== "string" || !(aspect in ASPECTS)) throw new StudioError(`bad --aspect "${aspect}"`, `use one of ${Object.keys(ASPECTS).join(", ")}`);
	const cuts = film.cuts.filter((c) => c.aspect === (aspect as AspectId));
	if (cuts.length === 0) throw new StudioError(`film "${film.id}" has no ${aspect} cut`, `cuts: ${film.cuts.map((c) => c.aspect).join(", ")}`);
	return cuts;
}
