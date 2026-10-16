/**
 * Mouse handling for the GPU terminal: selection (click, drag, multi-click, block), ⌘-hover and
 * ⌘-click on URLs, click-to-move, wheel scroll. The renderer owns the selection; this only feeds
 * it cells and decides what a press means.
 */

import { invoke } from "@tauri-apps/api/core";
import { appliedZoom } from "#lib/zoom.ts";
import { cellAt, cellFromStats, wheelLines, type Cell, type CellHit } from "./geometry.ts";
import { mouseRoute, wheelReports } from "./mouseRoute.ts";

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
		void invoke("term_mouse", {
			tab,
			action,
			button,
			col: c.col,
			row: c.row,
			shift: e.shiftKey,
			alt: e.altKey,
			ctrl: e.ctrlKey,
		}).catch(quiet);
	}

	async #openUnder(tab: string, c: CellHit): Promise<boolean> {
		const at = { tab, col: c.col, row: c.row };
		const url = await invoke<string | null>("term_hyperlink_at", at).catch(() => null);
		if (url) {
			this.#o.onLink(url, true);
			return true;
		}
		const token = await invoke<string | null>("term_path_at", at).catch(() => null);
		return token ? this.#o.onFile(token) : false;
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
			this.#report(tab, "press", e.button, c, e);
			return;
		}
		if (e.button !== 0) return;
		if (e.metaKey && (await this.#openUnder(tab, c))) {
			this.#hovered = false;
			void invoke("term_hover", { tab }).catch(quiet);
			return;
		}
		this.#selecting = true;
		// A plain single click may be a click-to-move; a drag or a multi-click is a selection.
		this.#press =
			e.detail === 1 && !e.altKey && !e.shiftKey && !e.metaKey ? { col: c.col, row: c.row } : null;
		void invoke("term_select_start", {
			tab,
			col: c.col,
			row: c.row,
			frac: c.frac,
			clicks: Math.min(e.detail, 3),
			block: e.altKey,
		}).catch(quiet);
	};

	mousemove = (e: MouseEvent) => {
		if (this.#program !== null) {
			const c = this.#toCell(e);
			if (c && (c.col !== this.#lastCol || c.row !== this.#lastRow)) {
				this.#lastCol = c.col;
				this.#lastRow = c.row;
				this.#report(this.#o.tab(), "motion", this.#program, c, e);
			}
			return;
		}
		if (!this.#selecting) return this.#hoverUrl(e);
		if (!(e.buttons & 1)) return void this.mouseup(e);
		const c = this.#toCell(e);
		if (c && this.#press && (c.col !== this.#press.col || c.row !== this.#press.row)) this.#press = null;
		if (c) this.#update(c);
	};

	mouseup = async (e: MouseEvent) => {
		if (this.#program !== null) {
			const button = this.#program;
			this.#program = null;
			const c = this.#toCell(e);
			if (c) this.#report(this.#o.tab(), "release", button, c, e);
			return;
		}
		if (!this.#selecting) return;
		this.#selecting = false;
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
		const lines = wheelLines(e.deltaY, this.#cell);
		const notch = wheelReports(lines);
		if (notch && !this.#o.readonly() && (await this.#owns(tab, e))) {
			const c = this.#toCell(e);
			if (c) {
				for (let i = 0; i < notch.count; i++) this.#report(tab, notch.action, null, c, e);
				return;
			}
		}
		if (lines) void invoke("term_scroll", { tab, delta: lines }).catch(quiet);
	};

	#hoverUrl(e: MouseEvent) {
		if (!e.metaKey) return this.clearHover();
		const c = this.#toCell(e);
		if (!c) return;
		const key = `${c.col},${c.row}`;
		if (key === this.#lastHover) return;
		this.#lastHover = key;
		this.#hovered = true;
		void invoke("term_hover", { tab: this.#o.tab(), col: c.col, row: c.row }).catch(quiet);
	}

	clearHover = () => {
		this.#lastHover = "";
		if (!this.#hovered) return;
		this.#hovered = false;
		void invoke("term_hover", { tab: this.#o.tab() }).catch(quiet);
	};
}
