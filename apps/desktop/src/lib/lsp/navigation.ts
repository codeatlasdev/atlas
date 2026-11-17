/**
 * Pure logic of "go to definition": which click counts, what a server's reply means, where a file
 * lives (worktree or outside), which identifier to underline, and the back/forward chain of jump
 * positions. No DOM, no stores: `actions.ts` / `extension.ts` / `workspace.ts` are the glue.
 */
import { EditorSelection, EditorState, type Text } from "@codemirror/state";
import { posToOffset, uriToPath, type Position } from "#lib/lsp/convert.ts";

// --- the click ----------------------------------------------------------------------------------

interface Mods {
	metaKey: boolean;
	ctrlKey: boolean;
	altKey: boolean;
	shiftKey: boolean;
}

/** ⌘ on macOS, Ctrl elsewhere, and nothing else held (⌥ adds a cursor, ⇧ extends the selection). */
export function isGoToModifier(e: Mods, mac: boolean): boolean {
	if (e.altKey || e.shiftKey) return false;
	return mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
}

export const isGoToClick = (e: Mods & { button: number }, mac: boolean) =>
	e.button === 0 && isGoToModifier(e, mac);

// --- the reply ----------------------------------------------------------------------------------

/** A place a server pointed at: absolute path and the exact range (0-based LSP positions). */
export interface Target {
	path: string;
	start: Position;
	end: Position;
}

function isPos(p: unknown): p is Position {
	return (
		!!p &&
		typeof p === "object" &&
		"line" in p &&
		"character" in p &&
		Number.isInteger(p.line) &&
		Number.isInteger(p.character)
	);
}

function isRange(r: unknown): r is { start: Position; end: Position } {
	return !!r && typeof r === "object" && "start" in r && "end" in r && isPos(r.start) && isPos(r.end);
}

/** `field` of `o` when it is a string. */
function str(o: object, field: string): string | null {
	const v: unknown = field in o ? Object.getOwnPropertyDescriptor(o, field)?.value : undefined;
	return typeof v === "string" ? v : null;
}

function range(o: object, field: string) {
	const v: unknown = field in o ? Object.getOwnPropertyDescriptor(o, field)?.value : undefined;
	return isRange(v) ? v : null;
}

/**
 * `Location | LocationLink | an array of either | null` → targets. A link selects the symbol's name
 * (`targetSelectionRange`), a location its range. Malformed entries and non-`file:` URIs (a `zipfile:`
 * or `jar:` source we cannot read) are dropped, never thrown on.
 */
export function toTargets(res: unknown): Target[] {
	const list = Array.isArray(res) ? res : res ? [res] : [];
	const out: Target[] = [];
	for (const l of list) {
		if (!l || typeof l !== "object") continue;
		const uri = str(l, "targetUri") ?? str(l, "uri");
		const r = range(l, "targetSelectionRange") ?? range(l, "range");
		if (!uri || !r || !uri.startsWith("file://")) continue;
		out.push({ path: uriToPath(uri), start: r.start, end: r.end });
	}
	return out;
}

/** Selection covering `start..end`, clamped to the document (a server may point past a line's end). */
export function selectionOf(doc: Text, start: Position, end: Position) {
	return EditorSelection.single(posToOffset(doc, start), posToOffset(doc, end)).main;
}

// --- where the file lives -----------------------------------------------------------------------

export type Resolved =
	{ kind: "worktree"; worktreeId: string; rel: string; readOnly?: true } | { kind: "external"; abs: string };

/** Collapse `.` and `..` segments: servers return paths like `/a/b/../c` (symlinked package stores). */
export function normalize(path: string): string {
	const out: string[] = [];
	for (const seg of path.split("/")) {
		if (seg === "..") out.pop();
		else if (seg !== "." && seg !== "") out.push(seg);
	}
	return `/${out.join("/")}`;
}

const DEPENDENCY = /(^|\/)node_modules\//;

/**
 * Deepest worktree whose root *contains* `abs` (a path-segment match: `/a/atlas-old` is not inside
 * `/a/atlas`). Dependency sources inside a worktree open there but read-only; files outside every
 * worktree (rustup, cargo registry, a global `node_modules`) are `external`.
 */
export function resolveTarget(target: string, worktrees: readonly { id: string; path: string }[]): Resolved {
	const abs = normalize(target);
	let best: { id: string; rel: string; len: number } | null = null;
	for (const w of worktrees) {
		const root = w.path.endsWith("/") ? w.path : `${w.path}/`;
		if (abs.startsWith(root) && (!best || root.length > best.len))
			best = { id: w.id, rel: abs.slice(root.length), len: root.length };
	}
	if (!best) return { kind: "external", abs };
	const r: Resolved = { kind: "worktree", worktreeId: best.id, rel: best.rel };
	return DEPENDENCY.test(best.rel) ? { ...r, readOnly: true } : r;
}

// --- ⌘-hover ------------------------------------------------------------------------------------

const WORD = /[\p{L}\p{N}_$]/u;

/** The identifier under `pos` (or touching it), for the ⌘-hover underline; `null` on anything else. */
export function hoverLink(state: EditorState, pos: number | null): { from: number; to: number } | null {
	if (pos === null) return null;
	const line = state.doc.lineAt(pos);
	const text = line.text;
	let i = pos - line.from;
	if (!(i < text.length && WORD.test(text[i])) && !(i > 0 && WORD.test(text[i - 1]))) return null;
	if (!(i < text.length && WORD.test(text[i]))) i -= 1;
	let a = i;
	let b = i + 1;
	while (a > 0 && WORD.test(text[a - 1])) a--;
	while (b < text.length && WORD.test(text[b])) b++;
	return { from: line.from + a, to: line.from + b };
}

// --- position history (⌃- / ⌃⇧-) ------------------------------------------------------------------

export interface NavPos {
	path: string;
	line: number;
	character: number;
}

const samePos = (a: NavPos | undefined, b: NavPos) =>
	!!a && a.path === b.path && a.line === b.line && a.character === b.character;

/**
 * Where the caret was before each jump, like VS Code's Go Back / Go Forward. `push` is called with the
 * position being left; `back`/`forward` take the position the user is at now so the other direction can
 * return to it.
 */
export class NavHistory {
	#back: NavPos[] = [];
	#fwd: NavPos[] = [];

	constructor(readonly cap = 100) {}

	get depth() {
		return this.#back.length;
	}
	canBack(): boolean {
		return this.#back.length > 0;
	}

	canForward(): boolean {
		return this.#fwd.length > 0;
	}

	push(from: NavPos) {
		this.#fwd = [];
		if (samePos(this.#back.at(-1), from)) return;
		this.#back.push(from);
		if (this.#back.length > this.cap) this.#back.shift();
	}

	back(current: NavPos): NavPos | null {
		const to = this.#back.pop();
		if (!to) return null;
		this.#fwd.push(current);
		return to;
	}

	forward(current: NavPos): NavPos | null {
		const to = this.#fwd.pop();
		if (!to) return null;
		this.#back.push(current);
		return to;
	}
}

// --- how an editor tab addresses its file --------------------------------------------------------

/**
 * A tab's `path` is relative to its worktree, or absolute for a file outside every worktree (the
 * Rust std library, the cargo registry, a global `node_modules`): relative paths never start with `/`.
 */
export function fileOfTab(root: string, path: string): { abs: string; external: boolean } {
	return path.startsWith("/") ? { abs: path, external: true } : { abs: `${root}/${path}`, external: false };
}

/** Registry key of a live editor (`symbols.ts`): the worktree that hosts the tab and its path. */
export function editorKey(worktreeId: string, path: string): string {
	return `${worktreeId}:${path}`;
}

/**
 * The tab that shows a resolved target. External files need a worktree to host the tab (`hostWorktree`,
 * the one the jump started from); `null` when there is none. Everything outside the worktree is read-only.
 */
export function tabFor(
	r: Resolved,
	hostWorktree?: string,
): { worktreeId: string; path: string; readOnly: boolean } | null {
	if (r.kind === "worktree") return { worktreeId: r.worktreeId, path: r.rel, readOnly: r.readOnly === true };
	if (!hostWorktree) return null;
	return { worktreeId: hostWorktree, path: r.abs, readOnly: true };
}

/** Dependency and external sources: browsable (caret, selection, find, folding) but nothing can change the text. */
export function readOnlyExtension() {
	return [EditorState.readOnly.of(true), EditorState.transactionFilter.of((tr) => (tr.docChanged ? [] : tr))];
}
