/**
 * Themed find/replace panel replacing CodeMirror's default DOM: find + replace fields, case / whole
 * word / regex / in-selection toggles, an `n of m` counter (capped scan), prev/next/all, replace /
 * replace all and a muted preview of the current match's replacement.
 *
 * Pure parts (`countMatches`, `replacementPreview`) take a `SearchQuery` and a state/Text so they run
 * without a DOM.
 */
import {
	SearchQuery,
	closeSearchPanel,
	findNext,
	findPrevious,
	getSearchQuery,
	replaceAll,
	replaceNext,
	search,
	selectMatches,
	setSearchQuery,
} from "@codemirror/search";
import type { EditorState, Extension, Text } from "@codemirror/state";
import { EditorView, runScopeHandlers, type Panel, type ViewUpdate } from "@codemirror/view";
import { kbd } from "#lib/keys.ts";

/** The counter stops here and shows `10000+`. */
export const MATCH_CAP = 10_000;

export interface MatchCount {
	/** Matches found (at most `MATCH_CAP`). */
	total: number;
	capped: boolean;
	/** 1-based index of the match equal to `current`, or 0 when the selection is not a match. */
	index: number;
}

/**
 * Count matches of `query` in `doc` (inside `range` when given), stopping at `MATCH_CAP`.
 * `current` is the selected range; its match index is reported while scanning, no second pass.
 */
export function countMatches(
	query: SearchQuery,
	state: EditorState | Text,
	current?: { from: number; to: number },
	range?: { from: number; to: number },
): MatchCount {
	if (!query.valid) return { total: 0, capped: false, index: 0 };
	const cursor = query.getCursor(state, range?.from ?? 0, range?.to);
	let total = 0;
	let index = 0;
	for (let r = cursor.next(); !r.done; r = cursor.next()) {
		total++;
		if (current && r.value.from === current.from && r.value.to === current.to) index = total;
		if (total >= MATCH_CAP) return { total, capped: true, index };
	}
	return { total, capped: false, index };
}

/** `n of m` / `m+` / `No results` text of a count. */
export function counterLabel(c: MatchCount, valid: boolean, hasQuery: boolean): string {
	if (!hasQuery) return "";
	if (!valid) return "Invalid regex";
	if (!c.total) return "No results";
	const total = c.capped ? `${c.total}+` : `${c.total}`;
	return c.index ? `${c.index} of ${total}` : `${total} found`;
}

/**
 * The replacement text for the match under `sel` (the current selection) with `$1`, `$&` and `$$`
 * resolved for regex queries; `\n`, `\t`, `\r` escapes are unquoted like the real replace does.
 * `null` when there is no replace text or the selection is not a match.
 */
export function replacementPreview(
	query: SearchQuery,
	state: EditorState,
	sel: { from: number; to: number },
): string | null {
	if (!query.valid || !query.replace || sel.from === sel.to) return null;
	const cursor = query.getCursor(state, sel.from, sel.to);
	const r = cursor.next();
	if (r.done || r.value.from !== sel.from || r.value.to !== sel.to) return null;
	const unq = query.literal
		? query.replace
		: query.replace.replace(/\\([nrt\\])/g, (_, c: string) =>
				c === "n" ? "\n" : c === "r" ? "\r" : c === "t" ? "\t" : "\\",
			);
	if (!query.regexp) return unq;
	// RegExpCursor values carry the match array; the generic cursor type does not declare it.
	if (!("match" in r.value) || !Array.isArray(r.value.match)) return unq;
	const match: readonly (string | undefined)[] = r.value.match;
	return unq.replace(/\$([$&]|\d+)/g, (m, i: string) => {
		if (i === "&") return match[0] ?? "";
		if (i === "$") return "$";
		for (let l = i.length; l > 0; l--) {
			const n = +i.slice(0, l);
			if (n > 0 && n < match.length) return (match[n] ?? "") + i.slice(l);
		}
		return m;
	});
}

type El<K extends keyof HTMLElementTagNameMap> = HTMLElementTagNameMap[K];

function el<K extends keyof HTMLElementTagNameMap>(
	tag: K,
	attrs: Record<string, string> = {},
	...kids: (Node | string)[]
): El<K> {
	const e = document.createElement(tag);
	for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
	e.append(...kids);
	return e;
}

interface Toggle {
	key: "caseSensitive" | "wholeWord" | "regexp" | "inSelection";
	label: string;
	text: string;
	shortcut: string;
}

const TOGGLES: Toggle[] = [
	{ key: "caseSensitive", label: "Match case", text: "Aa", shortcut: kbd("alt-cmd-c") },
	{ key: "wholeWord", label: "Match whole word", text: "ab", shortcut: kbd("alt-cmd-w") },
	{ key: "regexp", label: "Use regular expression", text: ".*", shortcut: kbd("alt-cmd-r") },
	{ key: "inSelection", label: "Find in selection", text: "⊡", shortcut: kbd("alt-cmd-l") },
];

/** In-selection scope: set when the panel opens over a multi-line selection, kept until toggled. */
class FindPanel implements Panel {
	dom: HTMLElement;
	top = true;
	private query: SearchQuery;
	private find: HTMLInputElement;
	private repl: HTMLInputElement;
	private count: HTMLElement;
	private preview: HTMLElement;
	private toggles = new Map<Toggle["key"], HTMLButtonElement>();
	private replaceRow: HTMLElement;
	private chevron: HTMLButtonElement;
	private scope: { from: number; to: number } | null = null;
	private showReplace: boolean;
	private frame = 0;

	constructor(private view: EditorView) {
		this.query = getSearchQuery(view.state);
		const readOnly = view.state.readOnly;
		this.showReplace = !readOnly && !!this.query.replace;

		this.find = el("input", {
			class: "cm-textfield",
			name: "search",
			placeholder: "Find",
			"aria-label": "Find",
			"main-field": "true",
			spellcheck: "false",
			autocomplete: "off",
			form: "",
		});
		this.repl = el("input", {
			class: "cm-textfield",
			name: "replace",
			placeholder: "Replace",
			"aria-label": "Replace",
			spellcheck: "false",
			autocomplete: "off",
			form: "",
		});
		this.find.value = this.query.search;
		this.repl.value = this.query.replace;
		this.find.addEventListener("input", () => this.commit());
		this.repl.addEventListener("input", () => this.commit());

		this.count = el("span", { class: "cm-find-count", role: "status", "aria-live": "polite" });
		this.preview = el("div", { class: "cm-find-preview", "aria-live": "polite" });

		this.chevron = el(
			"button",
			{
				type: "button",
				class: "cm-find-btn cm-find-chevron",
				"aria-label": "Toggle replace",
				"aria-expanded": String(this.showReplace),
				title: `Toggle replace (${kbd("alt-cmd-f")})`,
			},
			"›",
		);
		this.chevron.addEventListener("click", () => this.setReplace(!this.showReplace));

		const toggleBtns = TOGGLES.map((t) => {
			const b = el(
				"button",
				{
					type: "button",
					class: "cm-find-btn cm-find-toggle",
					"aria-label": t.label,
					"aria-pressed": "false",
					title: `${t.label} (${t.shortcut})`,
				},
				t.text,
			);
			b.addEventListener("click", () => this.flip(t.key));
			this.toggles.set(t.key, b);
			return b;
		});

		const btn = (label: string, text: string, title: string, run: () => void) => {
			const b = el("button", { type: "button", class: "cm-find-btn", "aria-label": label, title }, text);
			b.addEventListener("click", run);
			return b;
		};

		const findRow = el(
			"div",
			{ class: "cm-find-row" },
			this.chevron,
			this.find,
			...toggleBtns,
			this.count,
			btn("Previous match", "↑", `Previous match (${kbd("shift-enter")})`, () => findPrevious(view)),
			btn("Next match", "↓", "Next match (Enter)", () => findNext(view)),
			btn("Select all matches", "≡", `Select all matches (${kbd("alt-enter")})`, () => selectMatches(view)),
			btn("Close", "×", "Close (Esc)", () => closeSearchPanel(view)),
		);
		this.replaceRow = el(
			"div",
			{ class: "cm-find-row cm-find-replace" },
			this.repl,
			btn("Replace", "Replace", "Replace (Enter)", () => replaceNext(view)),
			btn("Replace all", "All", `Replace all (${kbd("cmd-shift-enter")})`, () => replaceAll(view)),
		);
		this.replaceRow.hidden = !this.showReplace;
		this.chevron.hidden = readOnly;

		this.dom = el(
			"div",
			{ class: "cm-search cm-find", role: "search", "aria-label": "Find and replace" },
			findRow,
			this.replaceRow,
			this.preview,
		);
		this.dom.addEventListener("keydown", (e) => this.keydown(e));

		// Opening over a multi-line selection scopes the search to it, like VS Code.
		const m = view.state.selection.main;
		if (!m.empty && view.state.doc.lineAt(m.from).number !== view.state.doc.lineAt(m.to).number) {
			this.scope = { from: m.from, to: m.to };
			this.find.value = this.query.search;
		}
		this.sync();
	}

	mount() {
		this.find.select();
	}

	get pos() {
		return 80;
	}

	private setReplace(on: boolean) {
		this.showReplace = on;
		this.replaceRow.hidden = !on;
		this.chevron.setAttribute("aria-expanded", String(on));
		this.chevron.classList.toggle("cm-find-open", on);
		if (on) this.repl.focus();
		else this.find.focus();
	}

	private flip(key: Toggle["key"]) {
		if (key === "inSelection") {
			const m = this.view.state.selection.main;
			this.scope = this.scope ? null : m.empty ? null : { from: m.from, to: m.to };
			this.sync();
			return;
		}
		this.commit({ [key]: !this.query[key] });
	}

	private commit(over: Partial<Pick<SearchQuery, "caseSensitive" | "wholeWord" | "regexp">> = {}) {
		const q = new SearchQuery({
			search: this.find.value,
			replace: this.repl.value,
			caseSensitive: over.caseSensitive ?? this.query.caseSensitive,
			wholeWord: over.wholeWord ?? this.query.wholeWord,
			regexp: over.regexp ?? this.query.regexp,
		});
		if (q.eq(this.query)) return;
		this.query = q;
		this.view.dispatch({ effects: setSearchQuery.of(q) });
		this.sync();
	}

	private keydown(e: KeyboardEvent) {
		if (runScopeHandlers(this.view, e, "search-panel")) {
			e.preventDefault();
			return;
		}
		const mod = e.metaKey || e.ctrlKey;
		if (e.key === "Enter") {
			e.preventDefault();
			if (e.altKey) selectMatches(this.view);
			else if (e.target === this.repl) (mod && e.shiftKey ? replaceAll : replaceNext)(this.view);
			else (e.shiftKey ? findPrevious : findNext)(this.view);
			return;
		}
		if (mod && e.altKey) {
			const k = e.key.toLowerCase();
			const map: Record<string, Toggle["key"]> = {
				c: "caseSensitive",
				w: "wholeWord",
				r: "regexp",
				l: "inSelection",
			};
			// ⌥ changes `e.key` on macOS (⌥C = "ç"): fall back to the physical key.
			const code = e.code.startsWith("Key") ? e.code.slice(3).toLowerCase() : k;
			const t = map[code] ?? map[k];
			if (t) {
				e.preventDefault();
				this.flip(t);
			} else if (code === "f") {
				e.preventDefault();
				this.setReplace(!this.showReplace);
			}
		}
	}

	update(u: ViewUpdate) {
		for (const tr of u.transactions)
			for (const ef of tr.effects)
				if (ef.is(setSearchQuery) && !ef.value.eq(this.query)) {
					this.query = ef.value;
					this.find.value = ef.value.search;
					this.repl.value = ef.value.replace;
				}
		if (
			u.docChanged ||
			u.selectionSet ||
			u.transactions.some((t) => t.effects.some((e) => e.is(setSearchQuery)))
		)
			this.schedule();
	}

	private schedule() {
		if (this.frame) return;
		this.frame = requestAnimationFrame(() => {
			this.frame = 0;
			this.sync();
		});
	}

	destroy() {
		if (this.frame) cancelAnimationFrame(this.frame);
	}

	/** Refresh toggles, counter and replacement preview from the current state. */
	private sync() {
		const q = this.query;
		const state = this.view.state;
		const pressed: Record<Toggle["key"], boolean> = {
			caseSensitive: q.caseSensitive,
			wholeWord: q.wholeWord,
			regexp: q.regexp,
			inSelection: !!this.scope,
		};
		for (const [k, b] of this.toggles) {
			b.setAttribute("aria-pressed", String(pressed[k]));
			b.classList.toggle("cm-find-on", pressed[k]);
		}
		const sel = state.selection.main;
		const c = countMatches(q, state, sel, this.scope ?? undefined);
		this.count.textContent = counterLabel(c, q.valid, !!q.search);
		this.count.classList.toggle("cm-find-none", !!q.search && (!q.valid || !c.total));
		this.find.classList.toggle("cm-find-invalid", !!q.search && !q.valid);
		const p = this.showReplace ? replacementPreview(q, state, sel) : null;
		this.preview.hidden = p === null;
		this.preview.textContent =
			p === null ? "" : p === "" ? "Replaces with nothing" : `→ ${p.replace(/\n/g, "↵").replace(/\t/g, "→")}`;
	}
}

const FOCUS = "var(--atlas-focus-ring, var(--ca-brand-main))";

/** Structural + color rules of the panel (CSS vars only). */
export const findTheme = EditorView.theme({
	".cm-panel.cm-find": {
		display: "flex",
		flexDirection: "column",
		gap: "4px",
		padding: "6px 8px",
		color: "var(--ca-text-secondary)",
		fontSize: "12px",
	},
	".cm-find-row": { display: "flex", alignItems: "center", gap: "4px" },
	".cm-find-row .cm-textfield": { flex: "1 1 auto", minWidth: "120px", margin: 0, padding: "3px 6px" },
	".cm-find-row .cm-textfield.cm-find-invalid": { outline: "1px solid var(--ca-semantic-error)" },
	".cm-find-replace": { paddingLeft: "22px" },
	".cm-find-btn": {
		display: "inline-flex",
		alignItems: "center",
		justifyContent: "center",
		minWidth: "22px",
		height: "22px",
		padding: "0 5px",
		margin: 0,
		font: "inherit",
		lineHeight: 1,
		color: "var(--ca-text-secondary)",
		background: "transparent",
		border: "0.5px solid transparent",
		borderRadius: "5px",
		cursor: "default",
	},
	".cm-find-btn:hover": { background: "var(--ca-action-hover)", color: "var(--ca-text-primary)" },
	".cm-find-btn:focus-visible": { outline: `1px solid ${FOCUS}` },
	".cm-find-toggle": { fontFamily: "var(--mono)", fontSize: "11px" },
	".cm-find-toggle.cm-find-on": {
		background: "var(--ca-action-selected)",
		color: "var(--ca-text-primary)",
		borderColor: "var(--ca-outline-default)",
	},
	".cm-find-chevron": { width: "18px", minWidth: "18px", padding: 0, transition: "transform 120ms" },
	".cm-find-chevron.cm-find-open": { transform: "rotate(90deg)" },
	".cm-find-count": {
		flex: "0 0 auto",
		minWidth: "64px",
		textAlign: "right",
		fontVariantNumeric: "tabular-nums",
		color: "var(--ca-text-secondary)",
		whiteSpace: "nowrap",
	},
	".cm-find-count.cm-find-none": { color: "var(--ca-semantic-error)" },
	".cm-find-preview": {
		paddingLeft: "22px",
		color: "var(--ca-text-disabled)",
		fontFamily: "var(--mono)",
		fontSize: "11px",
		overflow: "hidden",
		textOverflow: "ellipsis",
		whiteSpace: "pre",
	},
});

/** `search()` with the themed panel; include once per editor (replaces `searchKeymap`'s default panel). */
export const findPanel: Extension = [
	search({ top: true, createPanel: (view) => new FindPanel(view) }),
	findTheme,
];
