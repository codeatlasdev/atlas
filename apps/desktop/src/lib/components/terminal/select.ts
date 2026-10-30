/** Pure decisions behind the GPU terminal's selection and link hover; `pointer.ts` does the I/O. */

/** Interval of the autoscroll while a drag is held outside the grid. */
export const AUTOSCROLL_MS = 32;

/** Most lines one autoscroll tick moves, however far the pointer is. */
const MAX_LINES = 12;

/**
 * Lines to scroll per tick while a selection is dragged outside the grid (positive = up into history,
 * the sign `term_scroll` and `term_select_scroll` use). `0` inside `[top, bottom]`. One line right at the edge,
 * one more per cell of distance, capped; a row height of 0 (no cell size yet) scrolls nothing.
 */
export function autoscroll(y: number, top: number, bottom: number, rowH: number): number {
	if (!(rowH > 0)) return 0;
	if (y < top) return Math.min(MAX_LINES, 1 + Math.floor((top - y) / rowH));
	if (y > bottom) return -Math.min(MAX_LINES, 1 + Math.floor((y - bottom) / rowH));
	return 0;
}

/** What `term_link_at` answers: a URL (plain or OSC 8) or a path-shaped token the app must vouch for. */
export interface LinkAt {
	kind: "url" | "path";
	target: string;
}

/**
 * ⌘-hover: does the cell show a link (underline + hand), and should the underline include a path? A path
 * counts only when the app says it names a file of the worktree (`isFile`, which may ask the disk); otherwise
 * any `name.ext` word would light up. A check that fails is no link.
 */
export async function hoverDecision(
	link: LinkAt | null,
	isFile: (token: string) => boolean | Promise<boolean>,
): Promise<{ link: boolean; paths: boolean }> {
	if (!link) return { link: false, paths: false };
	if (link.kind === "url") return { link: true, paths: false };
	const file = await Promise.resolve(isFile(link.target)).catch(() => false);
	return { link: file, paths: file };
}

/** WebKit's click `detail` keeps counting past three; a fourth click is a line selection again. */
export const selectionClicks = (detail: number) => Math.min(Math.max(detail, 1), 3);
