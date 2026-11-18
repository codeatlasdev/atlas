/**
 * Status bar model: the items to show, left to right, with VS Code's compact wording. Pure; the
 * component only renders `text` and sets `title`. `kind` tells it which items are clickable
 * pickers (`action`), plain text (`info`) or a warning/error tint.
 */
import { list } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
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
	dockerfile: "Dockerfile",
	svelte: "Svelte",
	vue: "Vue",
	lua: "Lua",
};

/** Language names that are words rather than names (`Plain text`), so they follow the app's language. */
const WORD_LABELS: Record<string, () => string> = {
	jsonc: () => m.workbench_editor_lang_jsonc(),
	shellscript: () => m.workbench_editor_lang_shell_script(),
	shell: () => m.workbench_editor_lang_shell_script(),
	bash: () => m.workbench_editor_lang_shell_script(),
	plaintext: () => m.workbench_editor_lang_plain_text(),
	text: () => m.workbench_editor_lang_plain_text(),
};

const ENCODING_LABELS: Record<string, string> = {
	"utf-8": "UTF-8",
	utf8: "UTF-8",
	"utf-16le": "UTF-16 LE",
	"utf-16be": "UTF-16 BE",
};

/** The text of an encoding: a name (`UTF-8`), or the one that has a word in it (`UTF-8 with BOM`). */
const encodingLabel = (encoding: string): string => {
	const e = encoding.toLowerCase();
	if (e === "utf-8-bom") return m.workbench_editor_encoding_utf8_bom();
	return ENCODING_LABELS[e] ?? encoding.toUpperCase();
};

/** Display name of a language id (`typescript` → `TypeScript`); unknown ids are capitalised as is. */
export function languageLabel(langId: string): string {
	const id = langId.toLowerCase();
	if (Object.hasOwn(WORD_LABELS, id)) return WORD_LABELS[id]();
	const known = LANGUAGE_LABELS[id];
	if (known) return known;
	return langId.length > 0 ? langId[0].toUpperCase() + langId.slice(1) : m.workbench_editor_lang_plain_text();
}

/** `Ln 12, Col 5`, then `(3 selections)` and/or `(24 selected)` appended like VS Code. */
function position(i: StatusInput): { text: string; title: string } {
	const { count, chars } = i.selections;
	let text = m.workbench_loose_editor_ln_line_col_col({ line: i.line, col: i.col });
	if (count > 1) text += ` (${m.workbench_editor_selections({ count })})`;
	if (chars > 0) text += ` (${m.workbench_editor_selected_chars({ count: chars })})`;
	return { text, title: m.workbench_editor_goto_line() };
}

/** `Spaces: 2` or `Tab size: 4`. */
export function indentText(i: Pick<StatusInput["indent"], "style" | "size">): string {
	return i.style === "tab"
		? m.workbench_editor_indent_tab_size({ size: i.size })
		: m.workbench_editor_indent_spaces({ size: i.size });
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
			text: list([m.workbench_editor_errors({ count: errors }), m.workbench_editor_warnings({ count: warnings })]),
			title: m.workbench_editor_show_problems(),
			kind: errors > 0 ? "error" : "warning",
		});
	}
	items.push({
		id: "indent",
		text: indentText(i.indent),
		title: i.indent.detected ? m.workbench_editor_indent_detected() : m.workbench_editor_select_indentation(),
		kind: "action",
	});
	items.push({
		id: "encoding",
		text: encodingLabel(i.encoding),
		title: m.workbench_editor_select_encoding(),
		kind: "info",
	});
	items.push({
		id: "eol",
		text: i.eol === "mixed" ? m.workbench_editor_eol_mixed() : { lf: "LF", crlf: "CRLF", cr: "CR" }[i.eol],
		title: i.eol === "mixed" ? m.workbench_editor_eol_mixed_title() : m.workbench_editor_select_eol(),
		kind: i.eol === "mixed" ? "warning" : "action",
	});
	items.push({
		id: "language",
		text: i.language.label,
		title: m.workbench_editor_select_language({ id: i.language.id }),
		kind: "action",
	});
	if (i.lspChip) items.push({ id: "lsp", text: i.lspChip, title: m.workbench_editor_language_server(), kind: "action" });
	if (i.wrap) items.push({ id: "wrap", text: m.workbench_editor_wrap(), title: m.workbench_editor_wrap_on(), kind: "info" });
	if (i.tier !== "normal") {
		items.push({
			id: "tier",
			text: i.tier === "huge" ? m.workbench_editor_huge_file() : m.workbench_editor_large_file(),
			title: m.workbench_editor_tier_title(),
			kind: "warning",
		});
	}
	return items;
}
