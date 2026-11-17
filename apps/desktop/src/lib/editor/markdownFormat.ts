/**
 * The formatting bar of the light Markdown editor: bold, italic, heading, lists and link as edits of the CodeMirror
 * document. Each command takes the view, dispatches one transaction (so undo is one step and autosave sees an
 * ordinary edit) and returns true. Pure text work: no DOM, no stores.
 *
 * Every command is a toggle, the way TextEdit and Xcode's formatting shortcuts behave: applied to text that already
 * has the format it takes the format off.
 */
import {
	EditorSelection,
	type ChangeSpec,
	type EditorState,
	type SelectionRange,
	type Text,
} from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

/** The first mark is what a new run gets; the others are recognised in files that use them. */
const BOLD = ["**", "__"];
const ITALIC = ["_", "*"];

/** The char of `doc` at `i`, or "" outside the document. */
const charAt = (doc: Text, i: number) => (i >= 0 && i < doc.length ? doc.sliceString(i, i + 1) : "");

/** A one-char mark only counts when it is not part of a longer run (the `*` of `**x**` is not italic). */
const lone = (doc: Text, i: number, mark: string) => mark.length > 1 || charAt(doc, i) !== mark;

/** The mark that sits right outside `from..to` (`**` `x` `**`), or null. */
function markAround(doc: Text, from: number, to: number, marks: string[]): string | null {
	for (const m of marks) {
		const n = m.length;
		if (from - n < 0 || to + n > doc.length) continue;
		if (doc.sliceString(from - n, from) !== m || doc.sliceString(to, to + n) !== m) continue;
		if (lone(doc, from - n - 1, m) && lone(doc, to + n, m)) return m;
	}
	return null;
}

/** The mark that wraps the selected text itself (`**x**` selected as a whole), or null. */
function markInside(text: string, marks: string[]): string | null {
	for (const m of marks) {
		const n = m.length;
		if (text.length <= 2 * n || !text.startsWith(m) || !text.endsWith(m)) continue;
		if (n === 1 && (text[1] === m || text[text.length - 2] === m)) continue;
		return m;
	}
	return null;
}

type Edit = { changes: ChangeSpec; range: SelectionRange };

function toggleMark(state: EditorState, r: SelectionRange, marks: string[]): Edit {
	const text = state.doc.sliceString(r.from, r.to);
	const outer = markAround(state.doc, r.from, r.to, marks);
	if (outer) {
		const n = outer.length;
		return {
			changes: [
				{ from: r.from - n, to: r.from },
				{ from: r.to, to: r.to + n },
			],
			range: EditorSelection.range(r.from - n, r.to - n),
		};
	}
	const inner = markInside(text, marks);
	if (inner) {
		const bare = text.slice(inner.length, text.length - inner.length);
		return {
			changes: { from: r.from, to: r.to, insert: bare },
			range: EditorSelection.range(r.from, r.from + bare.length),
		};
	}
	const m = marks[0];
	return {
		changes: [
			{ from: r.from, insert: m },
			{ from: r.to, insert: m },
		],
		range: EditorSelection.range(r.from + m.length, r.to + m.length),
	};
}

/** Run `edit` on every selection range as one transaction. */
function perRange(view: EditorView, edit: (state: EditorState, r: SelectionRange) => Edit) {
	view.dispatch(
		view.state.changeByRange((r) => edit(view.state, r)),
		{
			userEvent: "input.format",
			scrollIntoView: true,
		},
	);
	return true;
}

/** `**bold**` around the selection (or an empty pair at the caret); on text that is bold already, takes it off. */
export const toggleBold = (view: EditorView) => perRange(view, (s, r) => toggleMark(s, r, BOLD));

/** `_italic_` around the selection; a file's own `*italic*` is recognised and taken off too. */
export const toggleItalic = (view: EditorView) => perRange(view, (s, r) => toggleMark(s, r, ITALIC));

const LINK_PLACEHOLDER = "https://";

/**
 * A link: the selection becomes the text and the address is left selected to type over. A selected URL becomes the
 * address instead, with the caret in the empty text. Nothing selected inserts `[link text](https://)` with the text
 * selected.
 */
export const insertLink = (view: EditorView) =>
	perRange(view, (s, r) => {
		const text = s.doc.sliceString(r.from, r.to);
		if (/^https?:\/\/\S+$/.test(text)) {
			return {
				changes: { from: r.from, to: r.to, insert: `[](${text})` },
				range: EditorSelection.cursor(r.from + 1),
			};
		}
		const label = text || "link text";
		const urlFrom = r.from + label.length + 3;
		return {
			changes: { from: r.from, to: r.to, insert: `[${label}](${LINK_PLACEHOLDER})` },
			range: text
				? EditorSelection.range(urlFrom, urlFrom + LINK_PLACEHOLDER.length)
				: EditorSelection.range(r.from + 1, r.from + 1 + label.length),
		};
	});

// ---- lines ------------------------------------------------------------------------------------------------------------

const BULLET = /^\s*[-*+]\s+/;
const ORDERED = /^\s*\d+[.)]\s+/;
const HEADING = /^#{1,6}\s+/;

interface Row {
	from: number;
	text: string;
}

/**
 * The lines the selection touches: all of them for a caret or one line (a list button on an empty line starts a list),
 * the non-blank ones for a block. A range ending at the very start of a line does not include that line.
 */
function rows(state: EditorState): Row[] {
	const seen = new Set<number>();
	const all: Row[] = [];
	for (const r of state.selection.ranges) {
		const first = state.doc.lineAt(r.from).number;
		const end = state.doc.lineAt(r.to);
		const last = r.to > r.from && end.from === r.to ? end.number - 1 : end.number;
		for (let n = first; n <= Math.max(first, last); n++) {
			if (seen.has(n)) continue;
			seen.add(n);
			const line = state.doc.line(n);
			all.push({ from: line.from, text: line.text });
		}
	}
	return all.length === 1 ? all : all.filter((r) => r.text.trim());
}

/** The list or heading marker a line starts with (indent included), or "" when it has none. */
const markerOf = (text: string) => (BULLET.exec(text) ?? ORDERED.exec(text) ?? HEADING.exec(text))?.[0] ?? "";

/** Replace what each row starts with, without its indent when it is a list item: one transaction. */
function setLeads(view: EditorView, rs: Row[], lead: (row: Row, i: number) => string, keepIndent: boolean) {
	if (!rs.length) return false;
	const changes: ChangeSpec[] = rs.map((row, i) => {
		const indent = keepIndent ? /^\s*/.exec(row.text)![0].length : 0;
		const marker = markerOf(row.text);
		return {
			from: row.from + indent,
			to: row.from + Math.max(marker.length, indent),
			insert: lead(row, i),
		};
	});
	view.dispatch({ changes, userEvent: "input.format", scrollIntoView: true });
	return true;
}

function toggleList(view: EditorView, ordered: boolean) {
	const rs = rows(view.state);
	const kind = ordered ? ORDERED : BULLET;
	const had = rs.length > 0 && rs.every((r) => kind.test(r.text));
	return setLeads(view, rs, (_, i) => (had ? "" : ordered ? `${i + 1}. ` : "- "), true);
}

export const toggleBullets = (view: EditorView) => toggleList(view, false);
export const toggleNumbered = (view: EditorView) => toggleList(view, true);

/** The `#` level of a line: 1 to 6, or 0 when it is not a heading. */
export const headingLevel = (text: string) => HEADING.exec(text)?.[0].trim().length ?? 0;

/** Make the touched lines level `level` headings (1 to 6); 0 makes them plain text. */
export function setHeading(view: EditorView, level: number) {
	return setLeads(view, rows(view.state), () => (level ? `${"#".repeat(level)} ` : ""), false);
}

/** The heading button: plain text, then H1, H2, H3, then back to plain text. */
export function cycleHeading(view: EditorView) {
	const first = rows(view.state)[0];
	const now = first ? headingLevel(first.text) : 0;
	return setHeading(view, now >= 3 ? 0 : now + 1);
}
