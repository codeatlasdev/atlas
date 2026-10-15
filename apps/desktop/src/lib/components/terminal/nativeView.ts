/**
 * Keeps the native terminal view (a child view painted above the webview) in step with its DOM
 * slot: opens it once, tracks bounds/visibility, and builds the color and pref payloads.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { bootMark } from "#lib/boot.ts";
import { gpu } from "#lib/stores/gpu.svelte.ts";
import { prefs } from "#lib/stores/prefs.svelte.ts";
import { themes } from "#lib/stores/theme.svelte.ts";
import { overlayOpen } from "../nativeSlot.ts";
import { fontCandidates, insetRect } from "./fonts.ts";

/** Theme color as #rrggbb (the kit uses oklch; a canvas normalizes any CSS color). */
function hex(el: Element, cssVar: string, fallback: string): string {
	const v = getComputedStyle(el).getPropertyValue(cssVar).trim() || fallback;
	const ctx = document.createElement("canvas").getContext("2d")!;
	ctx.fillStyle = v;
	ctx.fillRect(0, 0, 1, 1);
	const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
	return "#" + [r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("");
}

/** Terminal background (#rrggbb): the theme's, else the surface; also paints the DOM slot behind the padding. */
export function termBg(el: Element): string {
	return themes.terminal?.bg ?? hex(el, "--ca-surface-base", "#111111");
}

function termColors(el: Element) {
	const t = themes.terminal;
	return {
		fg: t?.fg ?? hex(el, "--ca-text-primary", "#e6e6e6"),
		bg: termBg(el),
		ansi16: t?.ansi16,
		cursor: t?.cursor ?? undefined,
		cursor_text: t?.cursorText ?? undefined,
		selection_bg: t?.selectionBg ?? undefined,
		selection_fg: t?.selectionFg ?? undefined,
		min_contrast: prefs.value.terminal.minimum_contrast,
	};
}

export function termPrefs() {
	const t = prefs.value.terminal;
	return {
		font_size: t.font_size,
		font_family: fontCandidates(t.font_family),
		font_thicken: t.font_thicken,
		scrollback: t.max_scroll_history_lines,
		cursor_shape: t.cursor_shape,
		cursor_blink: t.blinking,
		osc52: t.osc52,
		ligatures: t.ligatures,
	};
}

/** Requested font lists already announced, so one font change toasts once however many tabs react. */
const announcedFonts: Record<string, true> = {};
let fontListening = false;

/** Toast once per distinct requested list when the renderer fell back from the font the user asked for. */
export function listenFontFallback() {
	if (fontListening) return;
	fontListening = true;
	void listen<{ tab: string; requested: string[]; resolved: string }>("term://font", (e) => {
		const { requested, resolved } = e.payload;
		const key = requested.join("\n");
		if (!requested.length || announcedFonts[key]) return;
		announcedFonts[key] = true;
		toast.info(`Font "${requested[0]}" is not installed for the fast terminal; using ${resolved}.`);
	});
}

export class NativeView {
	opened = false;
	#failed = false;
	// Observers fire concurrently; serialize so `term_open` runs once before any `term_bounds`.
	#chain = Promise.resolve();

	constructor(
		private ids: () => { tab: string; pty: string },
		private area: () => HTMLElement | undefined,
		private visible: () => boolean,
	) {}

	sync = () => {
		this.#chain = this.#chain.then(() => this.#apply());
		return this.#chain;
	};

	async #apply() {
		const area = this.area();
		if (!area || this.#failed) return;
		const { tab, pty } = this.ids();
		const r = area.getBoundingClientRect();
		const { padding_x: px, padding_y: py } = prefs.value.terminal;
		const rect = insetRect(r, px, py);
		// The native view sits above the webview: park it under dialogs/palette.
		const show = this.visible() && r.width > 0 && r.height > 0 && !overlayOpen(r);
		try {
			if (!this.opened) {
				if (!show) return;
				this.opened = true;
				bootMark("term_open:begin");
				await invoke("term_open", { tab, pty, ...rect, ...termColors(area), prefs: termPrefs() });
				bootMark("term_open:done");
			}
			await invoke("term_bounds", { tab, ...rect, visible: show });
		} catch (e) {
			this.#failed = true;
			const msg = errText(e);
			// A host that predates the raw stream and could not be replaced: xterm.js until it
			// restarts. (The app normally upgrades it live on connect, see `host.rs`.)
			if (msg.includes("pty_attach_raw")) {
				if (!gpu.hostTooOld)
					toast.info("GPU terminal turns on after the terminal host restarts (Settings → Terminal).");
				gpu.hostTooOld = true;
			} else toast.error(msg);
		}
	}

	/** Recolor live after a theme switch. */
	recolor() {
		const area = this.area();
		if (this.opened && area)
			void invoke("term_recolor", { tab: this.ids().tab, ...termColors(area) }).catch(() => {});
	}

	/** Push `terminal.*` settings live: scrollback, cursor, OSC 52. */
	setPrefs() {
		const p = termPrefs();
		if (this.opened) void invoke("term_set_prefs", { tab: this.ids().tab, prefs: p }).catch(() => {});
	}
}
