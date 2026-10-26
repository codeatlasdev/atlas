/**
 * Keeps the native terminal view (an NSView BELOW the transparent webview) in step with its DOM slot: opens it
 * once, registers the slot with the per-frame layout batch (`termLayout.ts`) and builds the color and pref
 * payloads. The slot is a hole in the page while `presented` is true; overlays never touch the view.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { bootMark } from "#lib/boot.ts";
import { gpu } from "#lib/stores/gpu.svelte.ts";
import { prefs } from "#lib/stores/prefs.svelte.ts";
import { themes } from "#lib/stores/theme.svelte.ts";
import { nativeRect } from "#lib/zoom.ts";
import { contentRect, fontCandidates } from "./fonts.ts";
import { TermLayout, usable } from "./termLayout.ts";

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

/** The one layout batch of the window: every terminal's rect crosses IPC together, once per frame. */
export const termLayout = new TermLayout((cmd, args) => invoke(cmd, args));

/** `term://presented` `{tab, presented}`: the backend's word on whether a view is on screen under its slot. */
const presenters = new Map<string, (presented: boolean) => void>();
let presentedListening = false;
function listenPresented() {
	if (presentedListening) return;
	presentedListening = true;
	void listen<{ tab: string; presented: boolean }>("term://presented", (e) => {
		termLayout.presented(e.payload.tab, e.payload.presented);
		presenters.get(e.payload.tab)?.(e.payload.presented);
	});
}

export class NativeView {
	opened = false;
	#failed = false;
	#off: (() => void) | undefined;
	// `term_open` runs once, before the slot joins the batch.
	#opening: Promise<void> | undefined;

	constructor(
		private ids: () => { tab: string; pty: string },
		private area: () => HTMLElement | undefined,
		private visible: () => boolean,
		/** The native view is on screen under the slot (open the hole) or not (paint the slot). */
		private onPresented: (presented: boolean) => void = () => {},
	) {}

	/** Schedule a layout: measures every terminal at once and sends one batched call. */
	sync = () => {
		void this.#open().then(() => termLayout.request()).catch((e) => this.#fail(e));
	};

	#rect() {
		const area = this.area();
		if (!area) return null;
		const r = contentRect(area);
		// The view's frame is in window points, the slot in CSS px (they differ under layout zoom). The content box
		// is inside the slot's padding border: the same box the page cuts out of its backdrop (`holeLayers.ts`).
		return nativeRect({ x: r.left, y: r.top, w: r.width, h: r.height });
	}

	/** Open the native view once, at the first usable rect of a visible slot. */
	#open(): Promise<void> {
		if (this.opened || this.#failed) return Promise.resolve();
		const area = this.area();
		const r = this.#rect();
		if (!area || !this.visible() || !usable(r)) return Promise.resolve();
		return (this.#opening ??= (async () => {
			const { tab, pty } = this.ids();
			bootMark("term_open:begin");
			this.opened = true;
			try {
				await invoke("term_open", { tab, pty, ...r, ...termColors(area), prefs: termPrefs() });
			} catch (e) {
				this.opened = false;
				throw e;
			} finally {
				this.#opening = undefined;
			}
			bootMark("term_open:done");
			listenPresented();
			presenters.set(tab, this.onPresented);
			const off = termLayout.add({
				tab,
				measure: () => this.#rect(),
				visible: () => this.visible(),
				onPresented: this.onPresented,
				// The view is gone (its render thread ended): open it again over the same pty, which hostd
				// kept alive with its scrollback (it replays into the new view).
				onDropped: () => {
					this.#off?.();
					this.opened = false;
					this.sync();
				},
			});
			this.#off = () => {
				off();
				if (presenters.get(tab) === this.onPresented) presenters.delete(tab);
			};
		})());
	}

	#fail(e: unknown) {
		this.#failed = true;
		this.onPresented(false);
		const msg = errText(e);
		// A host that predates the raw stream and could not be replaced: xterm.js until it
		// restarts. (The app normally upgrades it live on connect, see `host.rs`.)
		if (msg.includes("pty_attach_raw")) {
			if (!gpu.hostTooOld)
				toast.info("GPU terminal turns on after the terminal host restarts (Settings → Terminal).");
			gpu.hostTooOld = true;
		} else toast.error(msg);
	}

	/** The slot goes away (unmount): leave the batch. The backend view is closed by `term_close`. */
	dispose() {
		this.#off?.();
		this.#off = undefined;
	}

	/**
	 * "Reload terminal": the backend replaces this tab's surface and renderer; the pty, screen and
	 * scrollback stay. No-op before the view is opened.
	 */
	async reload() {
		if (!this.opened) return;
		await invoke("term_reload", { tab: this.ids().tab });
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
