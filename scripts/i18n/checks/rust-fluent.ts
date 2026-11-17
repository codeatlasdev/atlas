// Fluent catalogs of the Rust crates (crates/*/locales/<lang>/*.ftl), surface `rust`: the shared catalog rules plus
// Fluent syntax (junk), unique ids, and the hardcoded-text ratchet over println!/eprintln!/bail!/anyhow! call sites.
import { checkSurface, statusSurface } from "../engine.ts";
import { loadFluent } from "../formats/fluent.ts";
import { RUST, runRatchet } from "../ratchets.ts";
import type { Checker, Finding } from "../types.ts";

const checker: Checker = {
	name: "rust-fluent",

	async run(ctx) {
		if (ctx.surface && ctx.surface !== "rust") return [];
		const out: Finding[] = [];
		const surface = loadFluent(ctx);
		if (surface) out.push(...checkSurface(ctx, surface));
		out.push(...runRatchet(ctx, RUST));
		return out;
	},

	async status(ctx) {
		const surface = ctx.surface && ctx.surface !== "rust" ? undefined : loadFluent(ctx);
		return surface ? statusSurface(ctx, surface) : [];
	},
};

export default checker;
