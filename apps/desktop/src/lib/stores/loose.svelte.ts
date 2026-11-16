import { ws } from "#lib/stores/workspace.svelte.ts";

/**
 * Files opened on their own, in the light editor (a `.md` from Finder, `atlas notes.md`, a drop): no project, no
 * worktree, no registry entry. They live only in this window's memory (nothing is saved, so closing Atlas forgets
 * them, like TextEdit).
 */
class Loose {
	/** Absolute paths in the order they were opened: the light editor's tab strip. */
	files = $state<string[]>([]);
	/** The same files, most recently shown first: the sidebar's "Files" group. */
	recent = $state<string[]>([]);
	/** The one shown (when `ws.view` is `file`). */
	active = $state<string | null>(null);
	/** Line to land on once the file's editor is up (`atlas notes.md:12`); taken once. */
	#line = new Map<string, number>();

	/** Show `path` in the light editor, opening it first when it is not open yet. */
	open(path: string, line?: number) {
		if (!this.files.includes(path)) this.files.push(path);
		this.recent = [path, ...this.recent.filter((p) => p !== path)];
		if (line) this.#line.set(path, line);
		this.active = path;
		ws.view = "file";
	}

	/** The line `open` asked for, once. */
	takeLine(path: string): number | undefined {
		const l = this.#line.get(path);
		this.#line.delete(path);
		return l;
	}

	/** Close one file; the next one in the strip is shown, or the project view when it was the last. */
	close(path: string) {
		const i = this.files.indexOf(path);
		if (i < 0) return;
		this.files.splice(i, 1);
		this.recent = this.recent.filter((p) => p !== path);
		this.#line.delete(path);
		if (this.active !== path) return;
		this.active = this.files[Math.min(i, this.files.length - 1)] ?? null;
		if (!this.active && ws.view === "file") ws.view = "worktree";
	}
}

export const loose = new Loose();
