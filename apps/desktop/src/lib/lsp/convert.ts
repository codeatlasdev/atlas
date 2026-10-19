/**
 * Pure LSP <-> CodeMirror conversions. CodeMirror documents index UTF-16 code units, the same unit
 * LSP positions use (`positionEncodings: ["utf-16"]`), so `character` is `offset - line.from`.
 */
import { ChangeSet, Text, type ChangeSpec } from "@codemirror/state";

export interface Position {
	line: number;
	character: number;
}
export interface Range {
	start: Position;
	end: Position;
}
export interface TextEdit {
	range: Range;
	newText: string;
}
/** LSP `TextDocumentContentChangeEvent` (incremental form). */
export interface ContentChange {
	range: Range;
	text: string;
}

export function offsetToPos(doc: Text, offset: number): Position {
	const line = doc.lineAt(Math.min(Math.max(0, offset), doc.length));
	return { line: line.number - 1, character: Math.min(Math.max(0, offset), doc.length) - line.from };
}

/** Offset of an LSP position; out-of-range lines/characters clamp (servers may send `character` past EOL). */
export function posToOffset(doc: Text, pos: Position): number {
	if (pos.line < 0) return 0;
	if (pos.line >= doc.lines) return doc.length;
	const line = doc.line(pos.line + 1);
	return line.from + Math.min(Math.max(0, pos.character), line.length);
}

export const rangeToOffsets = (doc: Text, r: Range): [number, number] => [
	posToOffset(doc, r.start),
	posToOffset(doc, r.end),
];

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

export interface WorkspaceEdit {
	changes?: Record<string, TextEdit[]>;
	documentChanges?: (
		| { textDocument: { uri: string }; edits: TextEdit[] }
		| { kind: "create" | "rename" | "delete"; uri?: string; oldUri?: string; newUri?: string }
	)[];
}

/** `file:///C:/x` → `C:/x` on Windows (the app keeps forward-slash paths); `file:///x` → `/x` elsewhere. */
export const uriToPath = (uri: string) =>
	decodeURIComponent(uri.replace(/^file:\/\//, "")).replace(/^\/([A-Za-z]:\/)/, "$1");
/** A Windows drive path gets the leading slash a file URI needs: `C:/x` → `file:///C:/x`. */
export const pathToUri = (path: string) =>
	"file://" +
	(/^[A-Za-z]:\//.test(path) ? "/" : "") +
	path.split("/").map(encodeURIComponent).join("/").replace(/%3A/g, ":");

/** Flatten a WorkspaceEdit to `path -> edits` plus the file operations it carries. */
export function planWorkspaceEdit(edit: WorkspaceEdit) {
	const files = new Map<string, TextEdit[]>();
	const ops: { kind: "create" | "rename" | "delete"; path?: string; from?: string; to?: string }[] = [];
	for (const [uri, edits] of Object.entries(edit.changes ?? {}))
		files.set(uriToPath(uri), [...(files.get(uriToPath(uri)) ?? []), ...edits]);
	for (const dc of edit.documentChanges ?? []) {
		if ("edits" in dc) {
			const p = uriToPath(dc.textDocument.uri);
			files.set(p, [...(files.get(p) ?? []), ...dc.edits]);
		} else if (dc.kind === "rename")
			ops.push({ kind: "rename", from: uriToPath(dc.oldUri!), to: uriToPath(dc.newUri!) });
		else ops.push({ kind: dc.kind, path: uriToPath(dc.uri!) });
	}
	return { files, ops };
}
