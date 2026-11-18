/** `.env` parser: dotenv semantics (export prefix, quotes, multiline values, inline comments). */
import { m } from "#lib/i18n/m.ts";

export type EnvEntry =
	| {
			kind: "var";
			key: string;
			value: string;
			quoted: boolean;
			line: number;
			end: number;
			comment?: string;
			exported: boolean;
	  }
	| { kind: "comment"; text: string; line: number }
	| { kind: "invalid"; raw: string; line: number; reason: string };

export interface EnvDoc {
	entries: EnvEntry[];
	/** Keys defined more than once, with every line number. */
	duplicates: Map<string, number[]>;
}

const KEY = /^[A-Za-z_][A-Za-z0-9_.-]*$/;

/** Unescape a double-quoted dotenv value. */
const unescape = (s: string) =>
	s.replace(
		/\\([nrt"\\$])/g,
		(_, c: string) => ({ n: "\n", r: "\r", t: "\t", '"': '"', "\\": "\\", $: "$" })[c]!,
	);

export function parseEnv(text: string): EnvDoc {
	const lines = text.replace(/\r\n?/g, "\n").split("\n");
	const entries: EnvEntry[] = [];
	for (let i = 0; i < lines.length; i++) {
		const raw = lines[i];
		const trimmed = raw.trim();
		if (!trimmed) continue;
		if (trimmed.startsWith("#")) {
			entries.push({ kind: "comment", text: trimmed.replace(/^#\s?/, ""), line: i + 1 });
			continue;
		}
		const assign = /^(export\s+)?([^=\s]+)\s*=\s?(.*)$/.exec(trimmed);
		if (!assign) {
			entries.push({ kind: "invalid", raw, line: i + 1, reason: m.viewers_devops_env_reason_expected() });
			continue;
		}
		const [, exp, key, rest0] = assign;
		if (!KEY.test(key)) {
			entries.push({
				kind: "invalid",
				raw,
				line: i + 1,
				reason: m.viewers_devops_env_reason_bad_name({ key }),
			});
			continue;
		}
		let rest = rest0.trimStart();
		const q = rest[0];
		if (q === '"' || q === "'" || q === "`") {
			// Quoted: may span lines until the closing quote.
			let body = rest.slice(1);
			let end = i;
			let close = findClose(body, q);
			while (close < 0 && end + 1 < lines.length) {
				end++;
				body += "\n" + lines[end];
				close = findClose(body, q);
			}
			if (close < 0) {
				entries.push({
					kind: "invalid",
					raw,
					line: i + 1,
					reason: m.viewers_devops_env_reason_unterminated(),
				});
				i = end;
				continue;
			}
			const value = body.slice(0, close);
			const after = body.slice(close + 1).trim();
			entries.push({
				kind: "var",
				key,
				value: q === '"' ? unescape(value) : value,
				quoted: true,
				line: i + 1,
				end: end + 1,
				comment: after.startsWith("#") ? after.replace(/^#\s?/, "") : undefined,
				exported: !!exp,
			});
			i = end;
			continue;
		}
		let comment: string | undefined;
		const hash = rest.search(/\s#/);
		if (hash >= 0) {
			comment = rest.slice(hash).replace(/^\s*#\s?/, "");
			rest = rest.slice(0, hash);
		}
		entries.push({
			kind: "var",
			key,
			value: rest.trimEnd(),
			quoted: false,
			line: i + 1,
			end: i + 1,
			comment,
			exported: !!exp,
		});
	}
	const seen = new Map<string, number[]>();
	for (const e of entries) if (e.kind === "var") seen.set(e.key, [...(seen.get(e.key) ?? []), e.line]);
	const duplicates = new Map([...seen].filter(([, v]) => v.length > 1));
	return { entries, duplicates };
}

/** Index of the closing quote (honouring `\"` in double quotes), -1 when absent. */
function findClose(body: string, q: string): number {
	for (let i = 0; i < body.length; i++) {
		if (body[i] === "\\" && q === '"') i++;
		else if (body[i] === q) return i;
	}
	return -1;
}

export interface EnvDiff {
	/** In the example but not in the file. */
	missing: string[];
	/** In the file but not in the example. */
	extra: string[];
}

/** Keys of `file` against `example` (e.g. `.env` vs `.env.example`). */
export function diffEnv(file: EnvDoc, example: EnvDoc): EnvDiff {
	const have = new Set(file.entries.flatMap((e) => (e.kind === "var" ? [e.key] : [])));
	const want = new Set(example.entries.flatMap((e) => (e.kind === "var" ? [e.key] : [])));
	return { missing: [...want].filter((k) => !have.has(k)), extra: [...have].filter((k) => !want.has(k)) };
}

/** Sibling file names to compare against, best first (`.env.local` → `.env.example`, `.env.sample`, …). */
export const ENV_EXAMPLES = [".env.example", ".env.sample", ".env.template", ".env.dist", "example.env"];

/** True when the key name suggests a secret (masked even for the "reveal public" shortcut). */
export const looksSecret = (key: string) =>
	/(SECRET|TOKEN|PASSWORD|PASSWD|PRIVATE|API_?KEY|CREDENTIAL|AUTH|SALT)/i.test(key);
