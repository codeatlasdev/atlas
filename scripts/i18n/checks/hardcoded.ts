// Hardcoded user-facing text, as two ratchets (rule `hardcoded`): Svelte templates (apps/desktop, site) and the format
// strings of println!/bail!/anyhow! in the user-facing crates. New offenders fail; see ratchets.ts and i18n/README.md.
import { RUST, SVELTE, runRatchet } from "../ratchets.ts";
import type { Checker } from "../types.ts";

const checker: Checker = {
	name: "hardcoded",
	async run(ctx) {
		return [...runRatchet(ctx, SVELTE), ...runRatchet(ctx, RUST)];
	},
};

export default checker;
