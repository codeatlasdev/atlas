/**
 * The CodeMirror-bound half of the LSP conversions (`convert.ts` is the pure half): a `ChangeSet` as LSP
 * incremental events, the debounce buffer that composes them, and TextEdits as CodeMirror changes. Loaded
 * with the editor, never at startup.
 */
import { ChangeSet, Text, type ChangeSpec } from "@codemirror/state";
import { offsetToPos, rangeToOffsets, type ContentChange, type TextEdit } from "#lib/lsp/convert.ts";

/**
 * LSP incremental changes for `changes` applied to `before`. Each event's range is expressed against
 * the document *as left by the previous events*, which is how servers apply them in order.
 */
export function toContentChanges(before: Text, changes: ChangeSet): ContentChange[] {
	const out: ContentChange[] = [];
	let doc = before;
	changes.iterChanges((fromA, toA, fromB, _toB, inserted) => {
		// Events apply in order, so each range is in the document left by the previous ones: the start
		// is the B-side offset (earlier edits already applied) and the span is the A-side length.
		const from = fromB;
		const to = fromB + (toA - fromA);
		out.push({
			range: { start: offsetToPos(doc, from), end: offsetToPos(doc, to) },
			text: inserted.toString(),
		});
		doc = ChangeSet.of({ from, to, insert: inserted }, doc.length).apply(doc);
	});
	return out;
}

/**
 * Pending changes of one document, merged while they wait for the debounce: the ChangeSets compose,
 * so a burst of keystrokes becomes the minimal set of events against the last-sent text.
 */
export class ChangeBuffer {
	#base: Text | null = null;
	#set: ChangeSet | null = null;

	get pending() {
		return this.#set !== null;
	}

	push(before: Text, changes: ChangeSet) {
		if (!this.#set || !this.#base) {
			this.#base = before;
			this.#set = changes;
		} else this.#set = this.#set.compose(changes);
	}

	/** The coalesced LSP events, or `[]` (and resets) when nothing is pending. */
	take(): ContentChange[] {
		if (!this.#set || !this.#base) return [];
		const out = toContentChanges(this.#base, this.#set);
		this.#base = this.#set = null;
		return out;
	}

	clear() {
		this.#base = this.#set = null;
	}
}

/** Apply LSP TextEdits (all against the same original text) to a document: returns the ChangeSpecs. */
export function editsToChanges(doc: Text, edits: TextEdit[]): ChangeSpec[] {
	return edits.map((e) => {
		const [from, to] = rangeToOffsets(doc, e.range);
		return { from, to: Math.max(from, to), insert: e.newText };
	});
}

/** Apply TextEdits to a plain string (files that are not open in an editor). */
export function applyTextEdits(text: string, edits: TextEdit[]): string {
	const doc = Text.of(text.split(/\r\n|\r|\n/));
	return ChangeSet.of(editsToChanges(doc, edits), doc.length).apply(doc).toString();
}
