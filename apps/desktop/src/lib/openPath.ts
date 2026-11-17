/** `atlas <path>` / `path_open`: which project a path belongs to and where it is inside it. Pure; the I/O is in ctl.ts. */

import { viewerKind, type ViewerKind } from "#lib/viewers/kind.ts";

export interface Located {
	/** Path of the file relative to the project root (forward slashes). */
	rel: string;
}

/** Forward slashes, no trailing slash: the form Atlas stores project roots in. */
export const slashed = (p: string) => p.replace(/\\/g, "/").replace(/(.)\/+$/, "$1");

/** `file` relative to `root` when it is inside it (case-insensitive on a Windows drive path), else `null`. */
export function relativeTo(root: string, file: string): string | null {
	const r = slashed(root);
	const f = slashed(file);
	const win = /^[a-z]:\//i.test(r);
	const [rc, fc] = win ? [r.toLowerCase(), f.toLowerCase()] : [r, f];
	if (fc === rc) return "";
	return fc.startsWith(`${rc}/`) ? f.slice(r.length + 1) : null;
}

/** The folder a file sits in (forward slashes); `/` for a file at the root. */
export const dirOf = (file: string) => slashed(file).replace(/\/[^/]*$/, "") || "/";

/** The file's own name. */
export const baseName = (file: string) => slashed(file).split("/").pop() ?? file;

/** The kinds the light editor shows: text (CodeMirror), Markdown (with its bar and preview) and SVG source. */
const LIGHT_KINDS: ViewerKind[] = ["text", "markdown", "svg"];

/**
 * Where an open request goes. A file the project check found no project for (`standalone`) opens in the light editor
 * when it is text; images, PDFs and media have no light viewer, so they open inside their folder (a plain folder: the
 * explorer and the full viewers, still no services). Everything else opens its project or folder as before.
 */
export function routeOpen(plan: { standalone: boolean }, file: string): "light-editor" | "project" {
	return plan.standalone && LIGHT_KINDS.includes(viewerKind(file)) ? "light-editor" : "project";
}

/**
 * What a file or folder dropped on the window does. Folders are added only when dropped on the sidebar (the projects
 * list, as before); a file opens wherever it lands.
 */
export function dropAction(kind: "file" | "dir", onSidebar: boolean): "add-folder" | "open-file" | "ignore" {
	if (kind === "file") return "open-file";
	return onSidebar ? "add-folder" : "ignore";
}
