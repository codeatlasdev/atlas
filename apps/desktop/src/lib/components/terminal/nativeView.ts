/**
 * The terminal's adapter over the native-view core (`lib/native/surface.ts`): how a GPU terminal is opened, placed and
 * closed, and the colour and pref payloads. Everything the native views share lives in the core.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { bootMark } from "#lib/boot.ts";
import { gpu } from "#lib/stores/gpu.svelte.ts";
import { prefs } from "#lib/stores/prefs.svelte.ts";
import { themes } from "#lib/stores/theme.svelte.ts";
import { createNativeSurface, type NativeRect, type NativeSurface } from "#lib/native/surface.ts";
import { fontCandidates, insetRect } from "./fonts.ts";
import { isViewDropped } from "./viewDropped.ts";

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

/**
 * The terminal's ADAPTER over the native-view core (`native/surface.ts`): the core serializes open → bounds → close,
 * carves the page transparent over the view, parks for swipes, strips 2 px for the loading bar and registers the view
 * for the reconciler; this class says how a GPU terminal is opened, placed and closed, and owns the terminal extras
 * (colours, prefs, reload).
 */
export class NativeView {
	#surface: NativeSurface;

	constructor(
		private ids: () => { tab: string; pty: string },
		private area: () => HTMLElement | undefined,
		private visible: () => boolean,
		/** Tauri's `invoke`; injected by the tests. */
		private call: typeof invoke = invoke,
		/** The colour payload of `term_open`; injected by the tests. */
		private colors: (el: Element) => Record<string, unknown> = termColors,
	) {
		const open = async (rect: NativeRect) => {
			const { tab, pty } = this.ids();
			const el = this.area();
			bootMark("term_open:begin");
			await this.call("term_open", {
				tab,
				pty,
				...rect,
				...(el ? this.colors(el) : {}),
				prefs: termPrefs(),
			});
			bootMark("term_open:done");
		};
		this.#surface = createNativeSurface({
			kind: "terminal",
			id: ids().tab,
			slot: area,
			visible,
			// The native view is the slot inset by the terminal's padding; the slot behind it paints the background.
			rect: (r) => {
				const { padding_x: px, padding_y: py } = prefs.value.terminal;
				return insetRect(r, px, py);
			},
			open,
			place: (rect, shown) => this.call("term_bounds", { tab: this.ids().tab, ...rect, visible: shown }),
			release: () =>
				this.call("term_close", { tab: this.ids().tab }).then(
					() => {},
					() => {},
				),
			// The native view is gone (its render thread ended): the core opens it again over the same pty, which hostd
			// kept alive with its scrollback (it replays into the new view).
			gone: isViewDropped,
			onError: (e) => {
				const msg = errText(e);
				// A host that predates the raw stream and could not be replaced: xterm.js until it restarts. (The app
				// normally upgrades it live on connect, see `host.rs`.)
				if (msg.includes("pty_attach_raw")) {
					if (!gpu.hostTooOld)
						toast.info("GPU terminal turns on after the terminal host restarts (Settings → Terminal).");
					gpu.hostTooOld = true;
				} else toast.error(msg);
				return "stop";
			},
			reload: () => this.call("term_reload", { tab: this.ids().tab }),
		});
	}

	get opened() {
		return this.#surface.isOpen();
	}

	sync = () => this.#surface.sync();

	/**
	 * The component is going away: close the view AFTER whatever is queued (an in-flight `term_open` included), and let
	 * nothing open or show it again. A close outside the chain landed before the open and left an owner-less view on
	 * screen over every other space.
	 */
	dispose = () => this.#surface.dispose();

	/**
	 * "Reload terminal": the backend replaces this tab's surface and renderer; the pty, screen and
	 * scrollback stay. No-op before the view is opened.
	 */
	reload() {
		return this.#surface.reload();
	}

	/** Recolor live after a theme switch. */
	recolor() {
		const area = this.area();
		if (this.opened && area)
			void this.call("term_recolor", { tab: this.ids().tab, ...termColors(area) }).catch(() => {});
	}

	/** Push `terminal.*` settings live: scrollback, cursor, OSC 52. */
	setPrefs() {
		const p = termPrefs();
		if (this.opened) void this.call("term_set_prefs", { tab: this.ids().tab, prefs: p }).catch(() => {});
	}
}
