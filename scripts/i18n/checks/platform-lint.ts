// Platform wording in source, as two ratchets (error on new offenders, existing ones in i18n/hardcoded-baseline.json):
//   shortcut        `⌘K`, `Ctrl+S` written into user-facing Svelte/TS/Rust: a shortcut is a {key} placeholder
//                   filled by keys.ts kbd()/label() or the Rust helper
//   platform-check  `navigator.platform` / `isMac =` anywhere but apps/desktop/src/lib/keys.ts
// The matching rules for catalog values (shortcut, platform-noun) live in engine.ts and run on every catalog.
import { PLATFORM_SNIFFING, SHORTCUTS, runRatchet } from "../ratchets.ts";
import type { Checker } from "../types.ts";

const checker: Checker = {
	name: "platform-lint",
	async run(ctx) {
		return [...runRatchet(ctx, SHORTCUTS), ...runRatchet(ctx, PLATFORM_SNIFFING)];
	},
};

export default checker;
