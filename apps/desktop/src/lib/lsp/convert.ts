/**
 * Pure LSP position/URI conversions, with no CodeMirror import (the shell's LSP bookkeeping reaches this at
 * startup; the CodeMirror-bound half is `changes.ts`). CodeMirror documents index UTF-16 code units, the same
 * unit LSP positions use (`positionEncodings: ["utf-16"]`), so `character` is `offset - line.from`.
 */

/** The part of CodeMirror's `Text` the conversions read: a `Text` satisfies it as is. */
export interface LineDoc {
	readonly length: number;
	readonly lines: number;
	lineAt(pos: number): { number: number; from: number };
	line(n: number): { from: number; length: number };
}

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

export function offsetToPos(doc: LineDoc, offset: number): Position {
	const line = doc.lineAt(Math.min(Math.max(0, offset), doc.length));
	return { line: line.number - 1, character: Math.min(Math.max(0, offset), doc.length) - line.from };
}

/** Offset of an LSP position; out-of-range lines/characters clamp (servers may send `character` past EOL). */
export function posToOffset(doc: LineDoc, pos: Position): number {
	if (pos.line < 0) return 0;
	if (pos.line >= doc.lines) return doc.length;
	const line = doc.line(pos.line + 1);
	return line.from + Math.min(Math.max(0, pos.character), line.length);
}

export const rangeToOffsets = (doc: LineDoc, r: Range): [number, number] => [
	posToOffset(doc, r.start),
	posToOffset(doc, r.end),
];

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
