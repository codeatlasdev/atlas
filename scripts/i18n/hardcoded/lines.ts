// Line scanners for two ratchets: modifier shortcuts written into user-facing source (`⌘K`, `Ctrl+S`: a shortcut is a
// placeholder filled by keys.ts kbd()/label() or the Rust helper), and ad hoc platform sniffing (`navigator.platform`,
// `isMac =`) outside keys.ts. Comment lines and test code are skipped; `i18n-ignore` on the line or the one above waives it.
import type { Found, Violation } from "../baseline.ts";
import { readText, rel, walk } from "../util.ts";
import { withoutTestTail } from "./rust.ts";

export const SHORTCUT = /[⌘⌃⌥⇧]|(?<![\p{L}])(?:Cmd|Command|Ctrl|Control|Shift|Alt|Option|Opt|Meta|Super)\s*\+/iu;
export const PLATFORM = /navigator\.platform|\bisMac\s*=/;

/** The one module that may spell shortcuts and read the platform. */
export const KEYS_MODULE = "apps/desktop/src/lib/keys.ts";

const COMMENT = /^\s*(?:\/\/|\/\*|\*|<!--|#)/;
const isTest = (p: string) => /\.test\.ts$|(?:^|\/)(?:tests?|__fixtures__)\//.test(p);

export function scanLines(root: string, dirs: string[], exts: string[], pattern: RegExp): Found {
	const found: Found = new Map();
	for (const dir of dirs) {
		for (const path of walk(`${root}/${dir}`, (p) => exts.some((e) => p.endsWith(e)))) {
			const file = rel(root, path);
			if (file === KEYS_MODULE || isTest(file)) continue;
			const text = file.endsWith(".rs") ? withoutTestTail(readText(path)) : readText(path);
			const lines = text.split("\n");
			const out: Violation[] = [];
			lines.forEach((l, i) => {
				if (COMMENT.test(l) || !pattern.test(l) || /i18n-ignore/.test(l) || /i18n-ignore/.test(lines[i - 1] ?? "")) return;
				out.push({ line: i + 1, text: l.trim().slice(0, 80) });
			});
			if (out.length) found.set(file, out);
		}
	}
	return found;
}

