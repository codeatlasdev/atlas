// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

/**
 * Regex matching Extended_Pictographic (real emojis) plus the Unicode symbol
 * ranges most commonly misused as inline icons:
 *   U+2190-U+24FF  arrows, math operators, misc technical, control pictures
 *   U+25A0-U+27FF  geometric shapes, misc symbols, dingbats
 *   U+2900-U+2BFF  supplemental arrows-B, misc math, misc symbols and arrows
 *
 * Box-drawing (U+2500-U+257F) and block elements (U+2580-U+259F) are excluded
 * because they are used in code comments as section separators, not as icons.
 * Braille patterns (U+2800-U+28FF) are also excluded.
 *
 * NOTE: this file must itself stay clean — use only \uXXXX escapes, no raw chars.
 */
const ICON_RE = /\p{Extended_Pictographic}|[\u2190-\u24FF\u25A0-\u27FF\u2900-\u2BFF]/u;

const ROOT = resolve(import.meta.dir, "../../../..");

/** Paths relative to apps/desktop/src */
const CHECKED_DIRS = [
	"lib/bots",
	"lib/shell",
	"lib/stores",
	"lib/components/BotsSettings.svelte",
	"routes/(app)/BotView.svelte",
];

const CHECKED_EXTS: Record<string, true> = { ".ts": true, ".svelte": true };

function collectFiles(dirOrFile: string): string[] {
	try {
		const stat = statSync(dirOrFile);
		if (stat.isFile()) return [dirOrFile];
		return readdirSync(dirOrFile).flatMap((entry) => {
			const full = join(dirOrFile, entry);
			const s = statSync(full);
			if (s.isDirectory()) return collectFiles(full);
			const ext = entry.slice(entry.lastIndexOf("."));
			return CHECKED_EXTS[ext] ? [full] : [];
		});
	} catch {
		return [];
	}
}

const srcRoot = join(ROOT, "apps", "desktop", "src");

const filesToCheck = CHECKED_DIRS.flatMap((rel) => collectFiles(join(srcRoot, rel)));

describe("bots UI: no emoji or pictograph icons in source files", () => {
	for (const file of filesToCheck) {
		const rel = file.replace(srcRoot + "/", "");
		test(rel, () => {
			const content = readFileSync(file, "utf-8");
			const lines = content.split("\n");
			const hits: string[] = [];
			for (let i = 0; i < lines.length; i++) {
				const line = lines[i];
				if (ICON_RE.test(line)) {
					// Reset lastIndex (global flag not set but be safe)
					hits.push(`  line ${i + 1}: ${line.trim().slice(0, 120)}`);
				}
			}
			expect(hits, `${rel} contains emoji/pictograph icon characters:\n${hits.join("\n")}`).toHaveLength(0);
		});
	}
});
