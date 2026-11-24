/**
 * The fingerprint of an error: the same bug reported twice, by two people, on two versions, has the same 16 hex
 * characters, so a toast burst folds into one, a crash file and an issue body can say it, and "Report issue" can
 * search for it (#320).
 *
 * A port of `crates/atlas-core/src/fingerprint.rs`, which is the source of truth: both are plain scans over ASCII
 * classes (no regexes, no `toLowerCase`, no `trim`), so Rust's Unicode-aware `\d`/`\b` and JavaScript's cannot
 * disagree, and both test against `__fixtures__/fingerprints.json`. Change one and the fixture, never one alone: the
 * fingerprint is already written in crash files and in issues.
 *
 * `fingerprint(message, stack)` hashes (FNV-1a, 64 bits) the normalized message and the key of the first frame
 * inside the app.
 */

/** Longest normalized message (in characters) that goes into the hash. */
const MAX_MESSAGE_CHARS = 200;

const isDigit = (c: string) => c >= "0" && c <= "9";
const isAlnum = (c: string) => isDigit(c) || (c >= "a" && c <= "z") || (c >= "A" && c <= "Z");
const isHex = (c: string) => isDigit(c) || (c >= "a" && c <= "f");

/** Trim space, tab, CR and LF only: `String.prototype.trim` also takes U+FEFF, U+00A0 and more, which Rust's does not. */
const trimWs = (s: string) => s.replace(/^[ \t\r\n]+/, "").replace(/[ \t\r\n]+$/, "");

/** ASCII-only lower case: `toLowerCase` also folds `İ` (to two characters) and Greek, which Rust's does not. */
const asciiLower = (s: string) => s.replace(/[A-Z]/g, (c) => String.fromCharCode(c.charCodeAt(0) + 32));

/** The fingerprint of an error: 16 lower-case hex characters. `stack` is a JavaScript stack, a Rust location, or empty. */
export function fingerprint(message: string, stack: string): string {
	const bytes = new TextEncoder().encode(`${normalizeMessage(message)}\n${frameKey(stack)}`);
	let h = 0xcbf29ce484222325n;
	for (const b of bytes) {
		h ^= BigInt(b);
		h = BigInt.asUintN(64, h * 0x100000001b3n);
	}
	return h.toString(16).padStart(16, "0");
}

/** What two reports of the same error have in common in their message. */
export function normalizeMessage(message: string): string {
	const first =
		message
			.split("\n")
			.map(trimWs)
			.find((l) => l !== "") ?? "";
	const words = asciiLower(first)
		.split(/[ \t\r\n]/)
		.filter((t) => t !== "")
		.map(pathToken);
	const masked = maskNumbers(maskHex(words.join(" ")));
	// Masking can leave a space (a path token that was only punctuation); collapse again.
	return Array.from(
		masked
			.split(" ")
			.filter((t) => t !== "")
			.join(" "),
	)
		.slice(0, MAX_MESSAGE_CHARS)
		.join("");
}

const OPEN = "(\"'`<[";
const CLOSE = ")\"'`>],;:";

/** A whitespace-free token; one that is a path (it has a `/` or `\`) becomes its file name. */
function pathToken(token: string): string {
	let start = 0;
	let end = token.length;
	while (start < end && OPEN.includes(token[start])) start++;
	while (end > start && CLOSE.includes(token[end - 1])) end--;
	const core = token.slice(start, end);
	if (!/[/\\]/.test(core)) return token;
	return stripPosition(core.slice(Math.max(core.lastIndexOf("/"), core.lastIndexOf("\\")) + 1));
}

/** `file.rs:12:5` -> `file.rs`; `file.rs:12` -> `file.rs`; anything else as it is. */
function stripPosition(s: string): string {
	let out = s;
	for (let n = 0; n < 2; n++) {
		const i = out.lastIndexOf(":");
		const tail = i < 0 ? "" : out.slice(i + 1);
		if (tail === "" || !/^[0-9]+$/.test(tail)) break;
		out = out.slice(0, i);
	}
	return out;
}

function isUuidAt(s: string, i: number): boolean {
	if (i + 36 > s.length) return false;
	for (let n = 0; n < 36; n++) {
		const c = s[i + n];
		if (n === 8 || n === 13 || n === 18 || n === 23 ? c !== "-" : !isHex(c)) return false;
	}
	return i + 36 >= s.length || !isAlnum(s[i + 36]);
}

/** UUIDs, `0x` numbers and runs of 8 or more hex digits (ids, hashes, addresses) become `#`. Input is lower case. */
function maskHex(s: string): string {
	let out = "";
	let i = 0;
	while (i < s.length) {
		if (!isAlnum(s[i])) {
			out += s[i++];
			continue;
		}
		if (isUuidAt(s, i)) {
			out += "#";
			i += 36;
			continue;
		}
		let end = i;
		while (end < s.length && isAlnum(s[end])) end++;
		const run = s.slice(i, end);
		const hexNumber = run.length > 2 && run.startsWith("0x") && [...run.slice(2)].every(isHex);
		out += hexNumber || (run.length >= 8 && [...run].every(isHex)) ? "#" : run;
		i = end;
	}
	return out;
}

const LABELS = ["-alpha.", "-beta.", "-rc."];

/**
 * Decimal numbers, with their dotted tails (`1.5.0`, `3.14`) and a release label after them (`-alpha.27`, `-beta.2`,
 * `-rc.1`), become `#`: the same bug on alpha.27 and on 1.6.0 is one bug.
 */
function maskNumbers(s: string): string {
	let out = "";
	let i = 0;
	while (i < s.length) {
		if (!isDigit(s[i])) {
			out += s[i++];
			continue;
		}
		for (;;) {
			while (i < s.length && isDigit(s[i])) i++;
			if (i + 1 < s.length && s[i] === "." && isDigit(s[i + 1])) i++;
			else break;
		}
		for (const label of LABELS) {
			if (s.startsWith(label, i) && i + label.length < s.length && isDigit(s[i + label.length])) {
				i += label.length;
				while (i < s.length && isDigit(s[i])) i++;
				break;
			}
		}
		out += "#";
	}
	return out;
}

interface Frame {
	fn: string;
	file: string;
	inApp: boolean;
}

/**
 * The `function|file` of the first frame of `stack` that is inside the app (a `.svelte`, `.ts`, `.js`, `.mjs` or `.rs`
 * file that is not a dependency), else of the first non-empty line; empty for no stack.
 */
export function frameKey(stack: string): string {
	const frames = stack
		.split("\n")
		.map(trimWs)
		.filter((l) => l !== "")
		.map(parseFrame);
	const f = frames.find((x) => x.inApp) ?? frames[0];
	return f ? `${f.fn}|${f.file}` : "";
}

function parseFrame(line: string): Frame {
	let fn: string;
	let loc: string;
	if (line.startsWith("at ")) {
		const rest = line.slice(3);
		const inner = rest.endsWith(")") ? rest.slice(0, -1) : null;
		const open = inner === null ? -1 : inner.lastIndexOf(" (");
		[fn, loc] = inner !== null && open >= 0 ? [inner.slice(0, open), inner.slice(open + 2)] : ["", rest];
	} else if (line.includes("@")) {
		const at = line.indexOf("@");
		[fn, loc] = [line.slice(0, at), line.slice(at + 1)];
	} else {
		[fn, loc] = ["", line];
	}
	fn = trimWs(fn);
	if (fn.startsWith("async ")) fn = fn.slice(6);
	if (fn.startsWith("new ")) fn = fn.slice(4);
	if (fn === "<anonymous>" || fn === "anonymous") fn = "";

	loc = stripPosition(trimWs(loc)).replaceAll("\\", "/");
	loc = loc.split("?")[0];
	const dependency =
		loc.includes("node_modules") || loc.includes(".cargo/registry") || loc.includes("/rustc/");
	const crates = loc.indexOf("crates/");
	const file = stripBundlerHash(crates >= 0 ? loc.slice(crates) : loc.slice(loc.lastIndexOf("/") + 1));
	const inApp = !dependency && [".svelte", ".ts", ".js", ".mjs", ".rs"].some((e) => file.endsWith(e));
	return { fn, file, inApp };
}

/** `index-AbCd1234.js` -> `index.js` (Vite names a chunk by its content hash, which changes every release). */
function stripBundlerHash(file: string): string {
	for (const ext of [".js", ".mjs"]) {
		if (!file.endsWith(ext)) continue;
		const stem = file.slice(0, -ext.length);
		const dash = stem.lastIndexOf("-");
		if (dash >= 0 && /^[A-Za-z0-9_]{8}$/.test(stem.slice(dash + 1))) return stem.slice(0, dash) + ext;
	}
	return file;
}
