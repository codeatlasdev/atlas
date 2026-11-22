// Hardcoded user-facing text, as three ratchets (rule `hardcoded`): Svelte templates (apps/desktop, site), the format
// strings of println!/bail!/anyhow! in the user-facing crates, and the rest of Rust's UI text (native menus and
// notifications, `.context("..")`, ratatui spans). New offenders fail; see ratchets.ts and i18n/README.md.
import { RUST, RUST_UI, SVELTE, runRatchet } from "../ratchets.ts";
import type { Checker } from "../types.ts";

const checker: Checker = {
	name: "hardcoded",
	async run(ctx) {
		return [...runRatchet(ctx, SVELTE), ...runRatchet(ctx, RUST), ...runRatchet(ctx, RUST_UI)];
	},
};

export default checker;
