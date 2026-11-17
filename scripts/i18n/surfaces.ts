// Every translatable surface of the repo (Paraglide apps and the Rust Fluent catalogs), loaded for the commands
// that edit them (i18n:fix, i18n:translate). `catalogs` is set for Paraglide surfaces: the file of each locale.
import { loadFluent } from "./formats/fluent.ts";
import { loadParaglide } from "./formats/paraglide.ts";
import type { Surface } from "./engine.ts";
import type { Ctx } from "./types.ts";

export interface Loaded {
	surface: Surface;
	catalogs?: Map<string, string>;
	descriptionsFile?: string;
}

export function loadSurfaces(ctx: Ctx): Loaded[] {
	const out: Loaded[] = loadParaglide(ctx).map((p) => ({ surface: p.surface, catalogs: p.catalogs, descriptionsFile: p.descriptionsFile }));
	const rust = ctx.surface && ctx.surface !== "rust" ? undefined : loadFluent(ctx);
	if (rust) out.push({ surface: rust });
	return ctx.surface ? out.filter((l) => l.surface.name === ctx.surface) : out;
}
