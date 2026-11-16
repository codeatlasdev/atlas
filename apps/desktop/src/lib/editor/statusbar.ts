/**
 * Status bar model: the items to show, left to right, with VS Code's compact wording. Pure; the
 * component only renders `text` and sets `title`. `kind` tells it which items are clickable
 * pickers (`action`), plain text (`info`) or a warning/error tint.
 */
import type { Eol } from "./detect.ts";
import type { Tier } from "./largeFile.ts";

export interface StatusInput {
	/** 1-based caret line and column. */
	line: number;
	col: number;
	selections: { count: number; chars: number; lines: number };
	indent: { style: "space" | "tab"; size: number; detected: boolean };
	eol: Eol | "mixed";
	encoding: string;
	language: { id: string; label: string };
	/** Language-server chip text (`tsserver`, `Starting…`), omitted when none. */
	lspChip?: string;
	problems: { errors: number; warnings: number };
	tier: Tier;
	wrap: boolean;
}

export interface StatusItem {
	id: string;
	text: string;
	title: string;
	kind: "info" | "action" | "warning" | "error";
}

const LANGUAGE_LABELS: Record<string, string> = {
	typescript: "TypeScript",
	typescriptreact: "TypeScript JSX",
	javascript: "JavaScript",
	javascriptreact: "JavaScript JSX",
	json: "JSON",
	jsonc: "JSON with Comments",
	html: "HTML",
	css: "CSS",
	scss: "SCSS",
	less: "Less",
	markdown: "Markdown",
	rust: "Rust",
	go: "Go",
	python: "Python",
	java: "Java",
	c: "C",
	cpp: "C++",
	csharp: "C#",
	ruby: "Ruby",
	php: "PHP",
	swift: "Swift",
	kotlin: "Kotlin",
	yaml: "YAML",
	toml: "TOML",
	xml: "XML",
	sql: "SQL",
	shellscript: "Shell Script",
	shell: "Shell Script",
	bash: "Shell Script",
	dockerfile: "Dockerfile",
	svelte: "Svelte",
	vue: "Vue",
	lua: "Lua",
	plaintext: "Plain Text",
	text: "Plain Text",
};

const ENCODING_LABELS: Record<string, string> = {
	"utf-8": "UTF-8",
	utf8: "UTF-8",
	"utf-16le": "UTF-16 LE",
	"utf-16be": "UTF-16 BE",
	"utf-8-bom": "UTF-8 with BOM",
};

const EOL_LABELS: Record<Eol | "mixed", string> = { lf: "LF", crlf: "CRLF", cr: "CR", mixed: "Mixed" };

/** Display name of a language id (`typescript` → `TypeScript`); unknown ids are capitalised as is. */
export function languageLabel(langId: string): string {
	const known = LANGUAGE_LABELS[langId.toLowerCase()];
	if (known) return known;
	return langId.length > 0 ? langId[0].toUpperCase() + langId.slice(1) : "Plain Text";
}

/** `Ln 12, Col 5`, then `3 selections` and/or `(24 selected)` appended like VS Code. */
function position(i: StatusInput): { text: string; title: string } {
	const { count, chars } = i.selections;
	let text = `Ln ${i.line}, Col ${i.col}`;
	if (count > 1) text += ` (${count} selections)`;
	if (chars > 0) text += ` (${chars} selected)`;
	return { text, title: "Go to line" };
}

/** `Spaces: 2` or `Tab Size: 4`. */
function indentText(i: StatusInput["indent"]): string {
	return i.style === "tab" ? `Tab Size: ${i.size}` : `Spaces: ${i.size}`;
}

/**
 * How soon an item gives way when the bar is too narrow (the component hides rank 3, then 2, then 1; rank 0 stays and
 * truncates instead). What the person acts on stays: where the caret is and what is wrong come first.
 */
export function statusRank(id: string): 0 | 1 | 2 | 3 {
	switch (id) {
		case "encoding":
		case "eol":
			return 3;
		case "indent":
		case "wrap":
			return 2;
		case "language":
		case "lsp":
			return 1;
		default:
			return 0;
	}
}

/** The status bar items for the current editor state, in display order. */
export function statusItems(i: StatusInput): StatusItem[] {
	const pos = position(i);
	const items: StatusItem[] = [{ id: "position", text: pos.text, title: pos.title, kind: "action" }];
	const { errors, warnings } = i.problems;
	if (errors > 0 || warnings > 0) {
		items.push({
			id: "problems",
			text: `${errors} ${errors === 1 ? "error" : "errors"}, ${warnings} ${warnings === 1 ? "warning" : "warnings"}`,
			title: "Show problems",
			kind: errors > 0 ? "error" : "warning",
		});
	}
	items.push({
		id: "indent",
		text: indentText(i.indent),
		title: i.indent.detected
			? "Indentation (detected from the file): select to change"
			: "Select indentation",
		kind: "action",
	});
	items.push({
		id: "encoding",
		text: ENCODING_LABELS[i.encoding.toLowerCase()] ?? i.encoding.toUpperCase(),
		title: "Select encoding",
		kind: "info",
	});
	items.push({
		id: "eol",
		text: EOL_LABELS[i.eol],
		title: i.eol === "mixed" ? "Mixed line endings: select to unify" : "Select end of line sequence",
		kind: i.eol === "mixed" ? "warning" : "action",
	});
	items.push({
		id: "language",
		text: i.language.label,
		title: `Select language mode (${i.language.id})`,
		kind: "action",
	});
	if (i.lspChip) items.push({ id: "lsp", text: i.lspChip, title: "Language server", kind: "action" });
	if (i.wrap) items.push({ id: "wrap", text: "Wrap", title: "Word wrap is on", kind: "info" });
	if (i.tier !== "normal") {
		items.push({
			id: "tier",
			text: i.tier === "huge" ? "Huge file" : "Large file",
			title: "Some editor features are off to keep this file fast",
			kind: "warning",
		});
	}
	return items;
}
