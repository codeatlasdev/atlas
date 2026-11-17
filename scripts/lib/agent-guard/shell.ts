// A small shell reader, enough for the guard: it splits a command line into simple commands (on `;`, `&&`, `||`, `|`, `&`,
// newlines and parentheses), removes quoting, collects the files a command redirects into, and reads inside `$(…)` and
// backticks. It is not a shell: variables and globs are left as written, which the callers treat as "cannot tell".

export interface Segment {
	/** The command and its arguments, quotes removed. */
	words: string[];
	/** Files this command writes with `>`, `>>`, `>|`, `&>`, `N>`. */
	writes: string[];
}

type Redirect = "write" | "skip" | "heredoc" | "dupfd";

const MAX_DEPTH = 4;

/** Index of the `)` closing the `(` before `from`, honoring quotes; `src.length` when unclosed. */
function closeParen(src: string, from: number): number {
	let depth = 1;
	for (let i = from; i < src.length; i++) {
		const c = src[i];
		if (c === "\\") i++;
		else if (c === "'") i = src.indexOf("'", i + 1) < 0 ? src.length : src.indexOf("'", i + 1);
		else if (c === '"') {
			for (i++; i < src.length && src[i] !== '"'; i++) if (src[i] === "\\") i++;
		} else if (c === "(") depth++;
		else if (c === ")" && --depth === 0) return i;
	}
	return src.length;
}

export function parseShell(src: string, depth = 0): Segment[] {
	const out: Segment[] = [];
	if (depth > MAX_DEPTH) return out;
	let words: string[] = [];
	let writes: string[] = [];
	let cur = "";
	let has = false;
	let mode: Redirect | undefined;
	let strip = false;
	const heredocs: { tag: string; strip: boolean }[] = [];

	const word = () => {
		if (!has) return;
		if (mode === "write") writes.push(cur);
		else if (mode === "dupfd") {
			if (!/^(\d+-?|-)$/.test(cur)) writes.push(cur);
		} else if (mode === "heredoc") heredocs.push({ tag: cur, strip });
		else if (mode !== "skip") words.push(cur);
		mode = undefined;
		cur = "";
		has = false;
	};
	const end = () => {
		word();
		if (words.length || writes.length) out.push({ words, writes });
		words = [];
		writes = [];
	};
	const nested = (inner: string) => {
		out.push(...parseShell(inner, depth + 1));
		has = true;
	};

	for (let i = 0; i < src.length; i++) {
		const c = src[i];
		const next = src[i + 1];
		if (c === "\\") {
			if (next === "\n") i++;
			else if (next !== undefined) {
				cur += next;
				has = true;
				i++;
			}
		} else if (c === "'") {
			const close = src.indexOf("'", i + 1);
			cur += src.slice(i + 1, close < 0 ? src.length : close);
			has = true;
			i = close < 0 ? src.length : close;
		} else if (c === '"') {
			has = true;
			for (i++; i < src.length && src[i] !== '"'; i++) {
				if (src[i] === "\\" && i + 1 < src.length) cur += src[++i];
				else if (src[i] === "$" && src[i + 1] === "(") {
					const close = closeParen(src, i + 2);
					nested(src.slice(i + 2, close));
					i = close;
				} else if (src[i] === "`") {
					const close = src.indexOf("`", i + 1);
					nested(src.slice(i + 1, close < 0 ? src.length : close));
					i = close < 0 ? src.length : close;
				} else cur += src[i];
			}
		} else if (c === "$" && next === "(") {
			const close = closeParen(src, i + 2);
			nested(src.slice(i + 2, close));
			i = close;
		} else if (c === "`") {
			const close = src.indexOf("`", i + 1);
			nested(src.slice(i + 1, close < 0 ? src.length : close));
			i = close < 0 ? src.length : close;
		} else if (c === "#" && !has) {
			while (i < src.length && src[i] !== "\n") i++;
			i--;
		} else if (c === ">" || c === "<" || (c === "&" && next === ">")) {
			if (next === "(") continue; // process substitution: `(` ends the word below
			if (has && /^\d+$/.test(cur) && mode === undefined) {
				cur = "";
				has = false;
			} else word();
			if (c === "<") {
				if (src.startsWith("<<<", i)) {
					mode = "skip";
					i += 2;
				} else if (next === "<") {
					mode = "heredoc";
					strip = src[i + 2] === "-";
					i += strip ? 2 : 1;
				} else mode = "skip";
			} else {
				if (c === "&") i++; // `&>`
				if (src[i + 1] === ">" || src[i + 1] === "|") i++;
				if (src[i + 1] === "&") {
					mode = "dupfd";
					i++;
				} else mode = "write";
			}
		} else if (c === "\n") {
			end();
			// Heredoc bodies are data, not commands.
			for (const h of heredocs.splice(0)) {
				let pos = i + 1;
				while (pos < src.length) {
					const nl = src.indexOf("\n", pos);
					const line = src.slice(pos, nl < 0 ? src.length : nl);
					pos = nl < 0 ? src.length : nl + 1;
					if ((h.strip ? line.replace(/^\t+/, "") : line) === h.tag) break;
				}
				i = pos - 1;
			}
		} else if (c === ";" || c === "|" || c === "&" || c === "(" || c === ")") {
			end();
			if ((c === "&" || c === "|") && next === c) i++;
		} else if (c === " " || c === "\t" || c === "\r") word();
		else {
			cur += c;
			has = true;
		}
	}
	end();
	return out;
}
