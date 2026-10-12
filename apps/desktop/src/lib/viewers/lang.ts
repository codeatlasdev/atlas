import type { Extension } from "@codemirror/state";
import type { Language } from "@codemirror/language";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { rust } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { yaml } from "@codemirror/lang-yaml";
import { extOf } from "#lib/viewers/kind.ts";

/** Fence names that differ from the file extension. */
const ALIAS: Record<string, string> = {
	typescript: "ts",
	javascript: "js",
	python: "py",
	rust: "rs",
	yml: "yaml",
	sh: "",
	bash: "",
};

/** CodeMirror language support for an extension or fence name (`ts`, `rust`, …), `[]` when unknown. */
export function langSupport(nameOrExt: string): Extension {
	const e = nameOrExt.toLowerCase();
	const ext = ALIAS[e] ?? e;
	if (["ts", "tsx", "js", "jsx", "mjs", "cjs", "svelte"].includes(ext))
		return javascript({ typescript: ext.startsWith("ts") || ext === "svelte", jsx: ext.endsWith("x") });
	if (ext === "json") return json();
	if (ext === "md" || ext === "markdown") return markdown();
	if (ext === "rs") return rust();
	if (ext === "py") return python();
	if (ext === "html" || ext === "htm") return html();
	if (ext === "css" || ext === "scss") return css();
	if (ext === "yaml") return yaml();
	return [];
}

export const langForPath = (path: string) => langSupport(extOf(path));

/** The parser behind a LanguageSupport extension, if any. */
export function languageOf(nameOrExt: string): Language | null {
	const s = langSupport(nameOrExt);
	return !Array.isArray(s) && "language" in s ? (s.language as Language) : null;
}
