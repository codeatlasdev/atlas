/**
 * ANSI escape handling for service logs: SGR (colors, bold, dim, italic, underline
 * in all five styles + color, strike, inverse; 16 / 256 / truecolor) becomes styled
 * runs, OSC 8 hyperlinks attach their URI to the runs they wrap, and every other
 * escape (cursor moves, erase, OSC titles, charset switches) is dropped. A carriage
 * return keeps only what follows it, the way a terminal overwrites a progress line.
 *
 * The 16 base colors are `var(--ansi-*)`: the theme store writes them from the
 * active theme's terminal palette, so logs match the terminal tabs.
 */

export type Underline = "single" | "double" | "curly" | "dotted" | "dashed";

export interface Style {
	fg?: string;
	bg?: string;
	bold?: boolean;
	dim?: boolean;
	italic?: boolean;
	/** `true` is the plain single underline (SGR 4); styles come from `4:n`, SGR 21 is double. */
	underline?: Underline;
	/** Underline color (SGR 58). */
	ulColor?: string;
	strike?: boolean;
	inverse?: boolean;
	/** OSC 8 target of the text. */
	link?: string;
}

export interface Run {
	text: string;
	style: Style | null;
}

const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/** CSS variable names of the 16 palette slots, in ANSI order (normal then bright). */
export const ANSI_VARS = [...NAMES, ...NAMES.map((n) => `bright-${n}`)].map((n) => `--ansi-${n}`);

function basic(n: number, bright: boolean): string {
	return `var(${ANSI_VARS[n + (bright ? 8 : 0)]})`;
}

function xterm256(n: number): string {
	if (n < 8) return basic(n, false);
	if (n < 16) return basic(n - 8, true);
	if (n < 232) {
		const i = n - 16;
		const v = (c: number) => (c ? c * 40 + 55 : 0);
		return `rgb(${v(Math.floor(i / 36))} ${v(Math.floor(i / 6) % 6)} ${v(i % 6)})`;
	}
	const g = (n - 232) * 10 + 8;
	return `rgb(${g} ${g} ${g})`;
}

const UL_STYLES: Underline[] = ["single", "single", "double", "curly", "dotted", "dashed"];

/** Extended color at `params[i]` (38/48/58): `5;n` or `2;r;g;b`. Returns [color, params consumed]. */
function extColor(params: number[], i: number): [string | undefined, number] {
	if (params[i + 1] === 5 && params[i + 2] !== undefined) return [xterm256(params[i + 2]), 2];
	if (params[i + 1] === 2 && params[i + 4] !== undefined)
		return [`rgb(${params[i + 2]} ${params[i + 3]} ${params[i + 4]})`, 4];
	return [undefined, 0];
}

/** Apply one SGR parameter list (already split on `;` / `:`). Returns a new style. */
function sgr(params: number[], s: Style, colon: number): Style {
	s = { ...s };
	if (params.length === 0) params = [0];
	for (let i = 0; i < params.length; i++) {
		const p = params[i];
		if (p === 0) {
			const { link } = s;
			s = link ? { link } : {};
		} else if (p === 1) s.bold = true;
		else if (p === 2) s.dim = true;
		else if (p === 3) s.italic = true;
		else if (p === 4) {
			// `4:3` (colon form): the sub-parameter picks the style; 0 turns it off.
			if (colon === i + 1 && params[i + 1] !== undefined) {
				const n = params[i + 1];
				if (n === 0) delete s.underline;
				else s.underline = UL_STYLES[n] ?? "single";
				i++;
			} else s.underline = "single";
		} else if (p === 7) s.inverse = true;
		else if (p === 9) s.strike = true;
		else if (p === 21) s.underline = "double";
		else if (p === 22) s.bold = s.dim = false;
		else if (p === 23) s.italic = false;
		else if (p === 24) delete s.underline;
		else if (p === 27) s.inverse = false;
		else if (p === 29) s.strike = false;
		else if (p >= 30 && p <= 37) s.fg = basic(p - 30, false);
		else if (p === 39) delete s.fg;
		else if (p >= 40 && p <= 47) s.bg = basic(p - 40, false);
		else if (p === 49) delete s.bg;
		else if (p === 59) delete s.ulColor;
		else if (p >= 90 && p <= 97) s.fg = basic(p - 90, true);
		else if (p >= 100 && p <= 107) s.bg = basic(p - 100, true);
		else if (p === 38 || p === 48 || p === 58) {
			const [color, used] = extColor(params, i);
			if (color) s[p === 38 ? "fg" : p === 48 ? "bg" : "ulColor"] = color;
			i += used;
		}
	}
	// `false` flags from 22/23/27/29 are noise: keep styles minimal so an unstyled run is `null`.
	for (const k of ["bold", "dim", "italic", "strike", "inverse"] as const) if (s[k] === false) delete s[k];
	return s;
}

// CSI … final byte | OSC … BEL/ST | two-byte escapes (charset, keypad…).
const ESC_RE = /\x1b(?:\[([0-?]*)[ -/]*([@-~])|\]([^\x07\x1b]*)(?:\x07|\x1b\\)?|[()][0-9A-Za-z]|[@-Z\\-_])/g;
const SPLIT = /[;:]/;

/** Most runs a single line may produce; a pathological colorized blob collapses past this. */
const MAX_RUNS = 512;

/** Only `http`, `https`, `file` and `mailto` OSC 8 targets are followed. */
const SAFE_LINK = /^(?:https?:\/\/|file:\/\/|mailto:)/i;

/** Parse one log line into styled runs. Lines without escapes cost one `includes`. */
export function parseAnsi(raw: string): Run[] {
	const line = lastCr(raw);
	if (!line.includes("\x1b")) return [{ text: stripControls(line), style: null }];

	const runs: Run[] = [];
	let style: Style = {};
	let at = 0;
	const push = (text: string) => {
		text = stripControls(text);
		if (!text) return;
		const st = Object.keys(style).length === 0 ? null : style;
		const prev = runs[runs.length - 1];
		if (prev && prev.style === st) prev.text += text;
		else if (runs.length < MAX_RUNS) runs.push({ text, style: st });
		else prev.text += text;
	};
	ESC_RE.lastIndex = 0;
	for (let m = ESC_RE.exec(line); m; m = ESC_RE.exec(line)) {
		push(line.slice(at, m.index));
		at = m.index + m[0].length;
		if (m[2] === "m") {
			const src = m[1] ?? "";
			const params = src
				.split(SPLIT)
				.filter((x) => x !== "")
				.map(Number);
			if (!params.some(Number.isNaN)) style = sgr(params, style, colonIndex(src));
		} else if (m[3] !== undefined && m[3].startsWith("8;")) {
			// OSC 8 ; params ; URI — an empty URI closes the link.
			const uri = m[3].slice(m[3].indexOf(";", 2) + 1);
			const { link: _drop, ...rest } = style;
			style = uri && SAFE_LINK.test(uri) ? { ...rest, link: uri } : rest;
		}
	}
	push(line.slice(at));
	return runs.length ? runs : [{ text: "", style: null }];
}

/**
 * Parameter index of a colon sub-parameter group start (`4:3` → 1), or -1. Only the
 * first colon group matters: `4:n` underline styles; 38/48/58 take `:` the same as `;`.
 */
function colonIndex(src: string): number {
	const parts = src.split(";");
	let idx = 0;
	for (const p of parts) {
		const sub = p.split(":");
		if (sub.length > 1 && sub[0] === "4") return idx + 1;
		idx += sub.length;
	}
	return -1;
}

/** Text after the last `\r` (progress-bar overwrite); a trailing CR (CRLF) is just dropped. */
function lastCr(raw: string): string {
	const cr = raw.lastIndexOf("\r");
	if (cr < 0) return raw;
	if (cr === raw.length - 1) {
		const prev = raw.lastIndexOf("\r", cr - 1);
		return prev < 0 ? raw.slice(0, -1) : raw.slice(prev + 1, -1);
	}
	return raw.slice(cr + 1);
}

/** Drop leftover C0 controls (BEL, backspace, a stray ESC) but keep tabs. */
function stripControls(s: string): string {
	// eslint-disable-next-line no-control-regex
	return /[\x00-\x08\x0b-\x1f\x7f]/.test(s) ? s.replace(/[\x00-\x08\x0b-\x1f\x7f]/g, "") : s;
}

/** Plain text of a raw log line: escapes and controls removed (the `line` the filters see). */
export function stripAnsi(raw: string): string {
	const line = lastCr(raw);
	if (!line.includes("\x1b")) return stripControls(line);
	return stripControls(line.replace(ESC_RE, ""));
}

const UL_CSS: Record<Underline, string> = {
	single: "solid",
	double: "double",
	curly: "wavy",
	dotted: "dotted",
	dashed: "dashed",
};

/** Inline CSS for a run (inverse swaps fg/bg; dim lowers opacity). */
export function css(s: Style | null): string {
	if (!s) return "";
	let fg = s.fg;
	let bg = s.bg;
	if (s.inverse) [fg, bg] = [bg ?? "var(--ca-surface-base)", fg ?? "var(--ca-text-primary)"];
	// Badges like `\e[41;97m ERROR \e[0m`: the theme's "white" can be dark on light
	// surfaces, so on a colored background force real white.
	if (bg && (!fg || /--ansi-(bright-)?white\)/.test(fg))) fg = "#fff";
	const out: string[] = [];
	if (fg) out.push(`color:${fg}`);
	if (bg) out.push(`background:${bg}`, "border-radius:2px");
	if (s.bold) out.push("font-weight:650");
	if (s.dim) out.push("opacity:.6");
	if (s.italic) out.push("font-style:italic");
	if (s.underline || s.strike) {
		out.push(
			`text-decoration-line:${[s.underline && "underline", s.strike && "line-through"].filter(Boolean).join(" ")}`,
		);
		if (s.underline) out.push(`text-decoration-style:${UL_CSS[s.underline]}`);
		if (s.ulColor) out.push(`text-decoration-color:${s.ulColor}`);
	}
	return out.join(";");
}
