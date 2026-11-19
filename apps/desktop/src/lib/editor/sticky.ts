/**
 * Sticky scroll: pins up to `MAX_STICKY` header lines of the scopes (function / class / impl /
 * interface / block declarations) that enclose the first visible line. Headers come from the Lezer
 * ancestors of that position (`resolveInner` + parents), never from a whole-document walk, and the
 * overlay updates on scroll through one `requestAnimationFrame` per frame.
 */
import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { m } from "#lib/i18n/m.ts";

export const MAX_STICKY = 3;

/** Same grammar node families as `lib/outline.ts` (kept local: that module installs app-wide `ops`). */
const DECL = /^(.+?)(Declaration|Definition|Item)$/;
/** Declarations that are not scopes worth pinning. */
const SKIP =
	/^(Import|Export|Use|ExternCrate|Variable|Lexical|Macro|Attribute|Field|Ambient|Let|Const|Static|Type|Property|Parameter|Bound)/;
/** A scope must span at least this many lines to be worth pinning. */
const MIN_LINES = 2;

/** Whether a Lezer node name is a pinnable declaration. */
export function isScopeNode(name: string): boolean {
	const m = DECL.exec(name);
	return !!m && !SKIP.test(m[1]);
}

/** A scope reduced to what the chain needs. */
export interface ScopeSpan {
	/** Start of the header line. */
	from: number;
	/** 1-based header line. */
	line: number;
	/** 1-based last line of the scope. */
	endLine: number;
	/** Header text (first line, trimmed). */
	text: string;
}

/** One pinned header. */
export interface StickyRow {
	from: number;
	line: number;
	text: string;
}

/**
 * Enclosing scopes of `topLine` (1-based) from an outermost-first list: a scope pins when its header
 * line is above `topLine` and it still covers it. Nested scopes starting on the same line collapse
 * to the innermost. The result is capped to the `max` innermost scopes, outermost first.
 */
export function stickyChain(scopes: readonly ScopeSpan[], topLine: number, max = MAX_STICKY): StickyRow[] {
	const out: StickyRow[] = [];
	for (const s of scopes) {
		if (s.line >= topLine || s.endLine < topLine || s.endLine - s.line < MIN_LINES - 1) continue;
		if (out.length && out[out.length - 1].line === s.line) out.pop();
		out.push({ line: s.line, text: s.text, from: s.from });
	}
	return out.slice(-max);
}

/** Pinnable scopes enclosing `pos`, outermost first, from the syntax tree. */
export function scopesAt(state: EditorState, pos: number): ScopeSpan[] {
	const found: ScopeSpan[] = [];
	let n = syntaxTree(state).resolveInner(pos, 1);
	for (;;) {
		if (isScopeNode(n.name)) {
			const first = state.doc.lineAt(n.from);
			found.push({
				from: first.from,
				line: first.number,
				endLine: state.doc.lineAt(n.to).number,
				text: first.text.trim(),
			});
		}
		if (!n.parent) break;
		n = n.parent;
	}
	return found.reverse();
}

class StickyScroll {
	dom: HTMLElement;
	private frame = 0;
	private sig = "";

	constructor(private view: EditorView) {
		this.dom = document.createElement("div");
		this.dom.className = "cm-sticky";
		this.dom.setAttribute("role", "navigation");
		this.dom.setAttribute("aria-label", m.workbench_editor_sticky_scopes());
		this.dom.hidden = true;
		// Mousedown (not click) so the editor keeps focus and the selection is not moved.
		this.dom.addEventListener("mousedown", (e) => this.onPress(e));
		view.scrollDOM.parentElement?.insertBefore(this.dom, view.scrollDOM);
		this.onScroll = this.onScroll.bind(this);
		view.scrollDOM.addEventListener("scroll", this.onScroll, { passive: true });
		this.schedule();
	}

	update(u: ViewUpdate) {
		if (
			u.docChanged ||
			u.viewportChanged ||
			u.geometryChanged ||
			syntaxTree(u.state) !== syntaxTree(u.startState)
		)
			this.schedule();
	}

	destroy() {
		if (this.frame) cancelAnimationFrame(this.frame);
		this.view.scrollDOM.removeEventListener("scroll", this.onScroll);
		this.dom.remove();
	}

	private onScroll() {
		this.schedule();
	}

	private schedule() {
		if (this.frame) return;
		this.frame = requestAnimationFrame(() => {
			this.frame = 0;
			this.render();
		});
	}

	private render() {
		const { view } = this;
		const top = view.scrollDOM.scrollTop;
		const block = view.lineBlockAtHeight(top - view.documentTop + view.scrollDOM.getBoundingClientRect().top);
		const topLine = view.state.doc.lineAt(block.from).number;
		const chain = stickyChain(scopesAt(view.state, block.from), topLine);
		const sig = chain.map((c) => `${c.line}:${c.text}`).join("\n");
		if (sig === this.sig) return;
		this.sig = sig;
		this.dom.hidden = chain.length === 0;
		this.dom.replaceChildren(
			...chain.map((c) => {
				const row = document.createElement("div");
				row.className = "cm-sticky-row";
				row.dataset.pos = String(c.from);
				row.title = m.viewers_devops_line({ line: c.line });
				row.textContent = c.text;
				return row;
			}),
		);
	}

	private onPress(e: MouseEvent) {
		const row = e.target instanceof HTMLElement ? e.target.closest<HTMLElement>(".cm-sticky-row") : null;
		if (!row?.dataset.pos) return;
		e.preventDefault();
		const pos = Number(row.dataset.pos);
		this.view.dispatch({
			selection: { anchor: pos },
			effects: EditorView.scrollIntoView(pos, { y: "start", yMargin: 0 }),
		});
		this.view.focus();
	}
}

const stickyPlugin = ViewPlugin.fromClass(StickyScroll);

const stickyTheme = EditorView.baseTheme({
	".cm-sticky": {
		position: "absolute",
		top: 0,
		left: 0,
		right: 0,
		zIndex: 5,
		display: "flex",
		flexDirection: "column",
		background: "var(--cm-sticky-bg, var(--ca-surface-base))",
		borderBottom: "0.5px solid var(--ca-outline-default)",
		boxShadow: "0 2px 6px color-mix(in oklab, black 18%, transparent)",
		fontFamily: "var(--mono)",
		pointerEvents: "auto",
	},
	".cm-sticky[hidden]": { display: "none" },
	".cm-sticky-row": {
		padding: "0 12px",
		height: "calc(var(--atlas-buffer-font-size, 12.5px) * var(--atlas-buffer-line-height, 1.55))",
		lineHeight: "calc(var(--atlas-buffer-font-size, 12.5px) * var(--atlas-buffer-line-height, 1.55))",
		whiteSpace: "pre",
		overflow: "hidden",
		textOverflow: "ellipsis",
		color: "var(--cm-fg, var(--ca-text-primary))",
		cursor: "default",
	},
	".cm-sticky-row:hover": { background: "var(--ca-action-hover)" },
});

/**
 * Sticky scroll overlay. The editor's host (`.cm-editor`) must be `position: relative`
 * (CodeMirror's own default); `theme.ts` positions it over the scroller.
 */
export const stickyScroll: Extension = [stickyPlugin, stickyTheme];
