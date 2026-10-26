/**
 * Mouse handling for the GPU terminal: selection (click, drag with autoscroll, multi-click, block, ⇧-extend),
 * ⌘-hover and ⌘-click on URLs, OSC 8 links and file paths, click-to-move, wheel scroll. The renderer owns the selection; this only feeds
 * it cells and decides what a press means.
 */

import { invoke } from "@tauri-apps/api/core";
import { appliedZoom } from "#lib/zoom.ts";
import { cellAt, cellFromStats, type Cell, type CellHit } from "./geometry.ts";
import { mouseRoute } from "./mouseRoute.ts";
import { wheelArgs } from "./wheel.ts";
import { AUTOSCROLL_MS, autoscroll, hoverDecision, selectionClicks, type LinkAt } from "./select.ts";

interface Options {
	tab: () => string;
	/** The element whose top-left is cell (0, 0). */
	area: () => HTMLElement | undefined;
	/** Focus the keyboard sink (the mousedown default would blur it). */
	focusInput: () => void;
	readonly: () => boolean;
	/** `terminal.click_to_move` / `terminal.copy_on_select`; padding insets cell (0, 0) from the area's corner. */
	prefs: () => { click_to_move: boolean; copy_on_select: boolean; padding_x: number; padding_y: number };
	onLink: (url: string, external: boolean) => void;
	/** A plain token was ⌘-clicked; `true` when it named a file and was opened. */
	onFile: (token: string) => boolean;
	/** Does this path-shaped token name a file of the worktree? Only such tokens light up on ⌘-hover. */
	isFile: (token: string) => boolean | Promise<boolean>;
	/** The pointer is over a live link (⌘ held): the slot shows the pointer hand meanwhile, until it is not. */
	onLinkMode?: (on: boolean) => void;
}

const quiet = () => {};

export class Pointer {
	/** Exact cell size reported by the renderer; refreshed per press (resizes change it). */
	#cell: Cell | null = null;
	#selecting = false;
	/** Cell of an in-progress plain click; cleared once the pointer leaves it. */
	#press: { col: number; row: number } | null = null;
	/** A URL underline is showing (cleared when ⌘ is released or the pointer leaves). */
	#hovered = false;
	/** Button of a press the program took (its drags and release go to it, not to the selection). */
	#program: number | null = null;
	#lastCol = -1;
	#lastRow = -1;
	#lastHover = "";
	/** Last pointer position in page px (a ⌘ press with a still mouse, and the autoscroll, read it). */
	#at: { x: number; y: number } | null = null;
	/** The autoscroll interval while a drag is held outside the grid. */
	#scroller: ReturnType<typeof setInterval> | undefined;
	/** Removes the window listeners of a drag in progress. */
	#unwatch: (() => void) | null = null;
	#o: Options;

	constructor(options: Options) {
		this.#o = options;
	}

	async #fetchCell() {
		const s = await invoke<Record<string, { cell_w: number; cell_h: number }>>("term_stats");
		this.#cell = cellFromStats(s[this.#o.tab()], appliedZoom());
	}

	#toCell(e: MouseEvent): CellHit | null {
		const area = this.#o.area();
		if (!area || !this.#cell) return null;
		const { padding_x, padding_y } = this.#o.prefs();
		const r = area.getBoundingClientRect();
		return cellAt(e.clientX, e.clientY, { left: r.left + padding_x, top: r.top + padding_y }, this.#cell);
	}

	#update(c: CellHit) {
		void invoke("term_select_update", { tab: this.#o.tab(), col: c.col, row: c.row, frac: c.frac }).catch(
			quiet,
		);
	}

	/** Does the program in this terminal own the mouse for this event (mode on, no ⇧/⌘)? */
	async #owns(tab: string, e: MouseEvent): Promise<boolean> {
		if (mouseRoute(true, e) !== "program") return false;
		return invoke<boolean>("term_mouse_captured", { tab, shift: e.shiftKey }).catch(() => false);
	}

	#report(tab: string, action: string, button: number | null, c: CellHit, e: MouseEvent) {
		// SGR-Pixels (DECSET 1016): the offset from the grid's corner in device px; the VT ignores it unless on.
		const [px, py] = this.#pixels(e);
		void invoke("term_mouse", {
			tab,
			action,
			button,
			col: c.col,
			row: c.row,
			shift: e.shiftKey,
			alt: e.altKey,
			ctrl: e.ctrlKey,
			px,
			py,
		}).catch(quiet);
	}

	/** The pointer's offset from cell (0, 0)'s corner in device px (negative in the margin). */
	#pixels(e: MouseEvent): [number, number] {
		const r = this.#o.area()?.getBoundingClientRect();
		const { padding_x, padding_y } = this.#o.prefs();
		const dpr = window.devicePixelRatio || 1;
		return [
			Math.round((e.clientX - (r?.left ?? 0) - padding_x) * dpr),
			Math.round((e.clientY - (r?.top ?? 0) - padding_y) * dpr),
		];
	}

	/** ⌘-click: a URL or OSC 8 link goes to the app (browser per Settings ▸ Links), a file token opens the editor. */
	async #openUnder(tab: string, c: CellHit): Promise<boolean> {
		const link = await invoke<LinkAt | null>("term_link_at", { tab, col: c.col, row: c.row }).catch(
			() => null,
		);
		if (!link) return false;
		if (link.kind === "url") {
			// `external` stays false: whether it opens in an Atlas tab or the system browser is the setting's call.
			this.#o.onLink(link.target, false);
			return true;
		}
		return this.#o.onFile(link.target);
	}

	mousedown = async (e: MouseEvent) => {
		const tab = this.#o.tab();
		// Only the left button selects; middle and right matter to a program that asked for the mouse.
		if (e.button > 2 || (e.button !== 0 && this.#o.readonly())) return;
		// Keep the textarea focused for keys/paste (the mousedown default would blur it).
		e.preventDefault();
		this.#o.focusInput();
		await this.#fetchCell().catch(quiet);
		const c = this.#toCell(e);
		if (!c) return;
		if (!this.#o.readonly() && e.button <= 2 && (await this.#owns(tab, e))) {
			this.#program = e.button;
			this.#lastCol = c.col;
			this.#lastRow = c.row;
			this.#watchWindow();
			this.#report(tab, "press", e.button, c, e);
			return;
		}
		if (e.button !== 0) return;
		if (e.metaKey && (await this.#openUnder(tab, c))) {
			this.clearHover();
			return;
		}
		this.#selecting = true;
		this.#watchWindow();
		this.#at = { x: e.clientX, y: e.clientY };
		// ⇧-click moves the end of the selection that is there (Ghostty, iTerm2, Terminal.app); a drag after it
		// keeps moving it. A plain single click may be a click-to-move; a drag or a multi-click is a selection.
		if (e.shiftKey && !e.altKey && !e.metaKey) {
			this.#press = null;
			void invoke("term_select_extend", { tab, col: c.col, row: c.row, frac: c.frac }).catch(quiet);
			return;
		}
		this.#press =
			e.detail === 1 && !e.altKey && !e.shiftKey && !e.metaKey ? { col: c.col, row: c.row } : null;
		void invoke("term_select_start", {
			tab,
			col: c.col,
			row: c.row,
			frac: c.frac,
			clicks: selectionClicks(e.detail),
			block: e.altKey,
		}).catch(quiet);
	};

	mousemove = (e: MouseEvent) => {
		if (this.#program !== null) return this.#programDrag(e);
		if (!this.#selecting) return this.#hoverUrl(e);
		this.#drag(e);
	};

	/** The button of a press the program took moved: report a new cell once. */
	#programDrag(e: MouseEvent) {
		const c = this.#toCell(e);
		if (!c || (c.col === this.#lastCol && c.row === this.#lastRow)) return;
		this.#lastCol = c.col;
		this.#lastRow = c.row;
		this.#report(this.#o.tab(), "motion", this.#program, c, e);
	}

	/** A selection drag moved (over the slot or, through the window listener, outside it). */
	#drag(e: MouseEvent) {
		if (!(e.buttons & 1)) return void this.mouseup(e);
		this.#at = { x: e.clientX, y: e.clientY };
		const c = this.#toCell(e);
		if (c && this.#press && (c.col !== this.#press.col || c.row !== this.#press.row)) this.#press = null;
		if (c) this.#update(c);
		this.#autoscroll();
	}

	/** Edges of the grid in page px: the pointer beyond them while dragging scrolls the viewport. */
	#gridEdges(): { top: number; bottom: number } | null {
		const r = this.#o.area()?.getBoundingClientRect();
		if (!r) return null;
		const { padding_y } = this.#o.prefs();
		return { top: r.top + padding_y, bottom: r.bottom - padding_y };
	}

	/** Start, keep or stop the interval that scrolls while the drag is held outside the grid. */
	#autoscroll() {
		const edges = this.#gridEdges();
		const lines = this.#scrollLines(edges);
		if (!lines) {
			clearInterval(this.#scroller);
			this.#scroller = undefined;
			return;
		}
		this.#scroller ??= setInterval(() => this.#scrollTick(), AUTOSCROLL_MS);
	}

	#scrollLines(edges: { top: number; bottom: number } | null): number {
		if (!edges || !this.#at || !this.#cell) return 0;
		return autoscroll(this.#at.y, edges.top, edges.bottom, this.#cell.h);
	}

	#scrollTick() {
		const lines = this.#scrollLines(this.#gridEdges());
		if (!this.#selecting || !lines || !this.#at) return this.#autoscroll();
		const c = this.#toCell({ clientX: this.#at.x, clientY: this.#at.y } as MouseEvent);
		void invoke("term_select_scroll", {
			tab: this.#o.tab(),
			lines,
			col: c?.col ?? 0,
			frac: c?.frac ?? 0,
		}).catch(quiet);
	}

	/**
	 * While a button is held the pointer may leave the slot (a selection dragged past the pane, a program's
	 * drag): the slot's own handlers would never see the release. These listen on the window for that case only;
	 * events over the slot reach its handlers, so they are skipped here.
	 */
	#watchWindow() {
		if (this.#unwatch) return;
		const outside = (e: MouseEvent) => !(e.target instanceof Node && this.#o.area()?.contains(e.target));
		const move = (e: MouseEvent) => outside(e) && this.mousemove(e);
		const up = (e: MouseEvent) => outside(e) && void this.mouseup(e);
		window.addEventListener("mousemove", move);
		window.addEventListener("mouseup", up);
		this.#unwatch = () => {
			window.removeEventListener("mousemove", move);
			window.removeEventListener("mouseup", up);
			this.#unwatch = null;
		};
	}

	/** The drag is over: no window listeners, no autoscroll. */
	#endDrag() {
		this.#unwatch?.();
		clearInterval(this.#scroller);
		this.#scroller = undefined;
	}

	mouseup = async (e: MouseEvent) => {
		if (this.#program !== null) {
			const button = this.#program;
			this.#program = null;
			this.#endDrag();
			const c = this.#toCell(e);
			if (c) this.#report(this.#o.tab(), "release", button, c, e);
			return;
		}
		if (!this.#selecting) return;
		this.#selecting = false;
		this.#endDrag();
		const tab = this.#o.tab();
		const c = this.#toCell(e);
		const click =
			this.#press && c && c.col === this.#press.col && c.row === this.#press.row ? this.#press : null;
		this.#press = null;
		if (c) this.#update(c);
		const t = this.#o.prefs();
		// Ghostty `cursor-click-to-move`: a click on the prompt line moves the shell's cursor there.
		if (click && t.click_to_move && !this.#o.readonly()) {
			const moved = await invoke<boolean>("term_click_move", { tab, col: click.col, row: click.row }).catch(
				() => false,
			);
			if (moved) return void invoke("term_clear_selection", { tab }).catch(quiet);
		}
		void invoke("term_select_end", { tab, copy: t.copy_on_select }).catch(quiet);
	};

	wheel = async (e: WheelEvent) => {
		e.preventDefault();
		const tab = this.#o.tab();
		// Before the first click the cell size is unknown: wheel at 16 px per row otherwise.
		if (!this.#cell) await this.#fetchCell().catch(quiet);
		const screen = this.#cell ? Math.floor((this.#o.area()?.clientHeight ?? 0) / this.#cell.h) : 24;
		const at = this.#toCell(e) ?? { col: 0, row: 0 };
		// One call per event: the host accumulates, routes (arrows / mouse reports / scrollback) and writes once.
		void invoke("term_wheel", { tab, e: wheelArgs(e, this.#cell, at, screen, this.#o.readonly()) }).catch(
			quiet,
		);
	};

	/** ⌘ is down over the link under the pointer? (`term_link_at`, then the app's say on paths.) */
	async #hoverAt(c: CellHit) {
		const tab = this.#o.tab();
		const link = await invoke<LinkAt | null>("term_link_at", { tab, col: c.col, row: c.row }).catch(
			() => null,
		);
		// The pointer moved on, or ⌘ went up, while the answer was in flight.
		if (this.#lastHover !== `${c.col},${c.row}`) return;
		const { link: show, paths } = await hoverDecision(link, this.#o.isFile);
		// ...and again after the (possibly disk) check.
		if (this.#lastHover !== `${c.col},${c.row}`) return;
		if (!show) return this.#unhover();
		this.#hovered = true;
		this.#o.onLinkMode?.(true);
		void invoke("term_hover", { tab, col: c.col, row: c.row, paths }).catch(quiet);
	}

	async #hoverUrl(e: MouseEvent) {
		this.#at = { x: e.clientX, y: e.clientY };
		if (!e.metaKey) return this.clearHover();
		// The cell size is fetched on a press; a ⌘-hover before the first click would otherwise find none.
		if (!this.#cell) await this.#fetchCell().catch(quiet);
		const c = this.#toCell(e);
		if (!c) return;
		const key = `${c.col},${c.row}`;
		if (key === this.#lastHover) return;
		this.#lastHover = key;
		await this.#hoverAt(c);
	}

	/** Drop the underline and the hand, keeping the "⌘ is down here" memo (the pointer is still over the slot). */
	#unhover() {
		if (!this.#hovered) return;
		this.#hovered = false;
		this.#o.onLinkMode?.(false);
		void invoke("term_hover", { tab: this.#o.tab() }).catch(quiet);
	}

	/**
	 * ⌘ went down or up while the pointer rests over the slot: links light up (or go out) without a mouse
	 * move, as in Ghostty. The window forwards the key; the pointer position is the last one seen.
	 */
	metaChanged = async (down: boolean) => {
		if (!down) return this.clearHover();
		if (!this.#cell) await this.#fetchCell().catch(quiet);
		if (!this.#at || !this.#o.area() || !this.#cell) return;
		const c = this.#toCell({ clientX: this.#at.x, clientY: this.#at.y } as MouseEvent);
		if (!c) return;
		this.#lastHover = `${c.col},${c.row}`;
		await this.#hoverAt(c);
	};

	clearHover = () => {
		this.#lastHover = "";
		this.#unhover();
	};

	/** Stop watching the window and the autoscroll (the terminal tab is going away). */
	dispose = () => {
		this.#endDrag();
		this.clearHover();
	};
}
