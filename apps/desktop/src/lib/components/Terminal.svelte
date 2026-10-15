<script lang="ts">
	import { onMount } from "svelte";
	import { Terminal } from "@xterm/xterm";
	import { FitAddon } from "@xterm/addon-fit";
	import { SearchAddon } from "@xterm/addon-search";
	import { WebLinksAddon } from "@xterm/addon-web-links";
	import { WebglAddon } from "@xterm/addon-webgl";
	import { Unicode11Addon } from "@xterm/addon-unicode11";
	import { listen } from "@tauri-apps/api/event";
	import { host } from "#lib/host.ts";
	import { b64bytes } from "#lib/b64.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import FindInput from "./FindInput.svelte";

	let {
		pty,
		focused = false,
		readonly = false,
		visible = true,
		onLink,
	}: {
		pty: string;
		focused?: boolean;
		/** False while the space is parked (a neighbour): the WebGL context is released, WebKit caps them. */
		visible?: boolean;
		readonly?: boolean;
		onLink: (url: string, external: boolean) => void;
	} = $props();

	let el: HTMLDivElement;
	let term: Terminal | undefined = $state.raw();
	let fit: FitAddon | undefined;
	let search: SearchAddon | undefined;
	let webgl: WebglAddon | undefined;
	/** Hidden until the first frame is drawn at the fitted size: no 80x24 flash, no DOM-to-WebGL swap, no blank replay. */
	let drawn = $state(false);
	let searchOpen = $state(false);
	let query = $state("");

	const CURSOR_STYLE = { block: "block", bar: "bar", underline: "underline", hollow: "block" } as const;

	/** Theme colors over the surface/text vars (WebGL can't draw a transparent canvas, so bg is always real). */
	function xtermColors() {
		const css = getComputedStyle(el);
		const bg = css.getPropertyValue("--ca-surface-base").trim() || css.backgroundColor;
		const fg = css.getPropertyValue("--ca-text-primary").trim() || css.color;
		const themed = Object.fromEntries(Object.entries(themes.xterm).filter(([, v]) => v !== undefined));
		return { background: bg, foreground: fg, cursor: fg, ...themed };
	}

	/** Terminal background behind the padding: the theme's, else the surface. */
	const hostBg = $derived(themes.xterm.background ?? "var(--ca-surface-base)");

	$effect(() => {
		if (focused && term) term.focus();
	});

	/**
	 * WebGL renderer. Loaded before `open` so the very first paint is WebGL (loading it after `open` paints with the
	 * DOM renderer first, then swaps). A lost context (WebKit drops the oldest when too many are live) disposes the
	 * addon, which falls back to the DOM renderer instead of leaving a black terminal.
	 */
	function loadWebgl(t: Terminal) {
		if (webgl) return;
		try {
			const addon = new WebglAddon();
			addon.onContextLoss(() => {
				addon.dispose();
				if (webgl === addon) webgl = undefined;
			});
			t.loadAddon(addon);
			webgl = addon;
		} catch {
			// DOM renderer fallback
		}
	}

	// Each mounted space keeps its terminals: only the visible one holds a WebGL context.
	$effect(() => {
		if (!term) return;
		if (visible) loadWebgl(term);
		else {
			webgl?.dispose();
			webgl = undefined;
		}
	});

	// A theme switch recolors open terminals; reads `themes.xterm` so the effect re-runs.
	$effect(() => {
		void themes.xterm;
		if (term) term.options.theme = xtermColors();
	});

	// `terminal.*` font and cursor settings apply live; the host refits to the new cell size.
	$effect(() => {
		const t = prefs.value.terminal;
		if (!term) return;
		term.options.fontFamily = t.font_family;
		term.options.fontSize = t.font_size;
		term.options.fontWeight = t.font_thicken ? 500 : "normal";
		term.options.fontWeightBold = t.font_thicken ? 700 : "bold";
		term.options.cursorStyle = CURSOR_STYLE[t.cursor_shape];
		term.options.cursorBlink = t.blinking;
		void t.padding_x;
		void t.padding_y;
		if (fit && el.offsetWidth && el.offsetHeight) {
			fit.fit();
			void host.ptyResize(pty, term.cols, term.rows).catch(() => {});
		}
	});

	onMount(() => {
		const t = new Terminal({
			allowProposedApi: true,
			scrollback: prefs.value.terminal.max_scroll_history_lines,
			fontFamily: prefs.value.terminal.font_family,
			fontSize: prefs.value.terminal.font_size,
			fontWeight: prefs.value.terminal.font_thicken ? 500 : "normal",
			fontWeightBold: prefs.value.terminal.font_thicken ? 700 : "bold",
			cursorBlink: prefs.value.terminal.blinking,
			cursorStyle: CURSOR_STYLE[prefs.value.terminal.cursor_shape],
			disableStdin: readonly,
			macOptionIsMeta: true,
			theme: xtermColors(),
		});
		term = t;
		const addon = new FitAddon();
		fit = addon;
		search = new SearchAddon();
		t.loadAddon(addon);
		t.loadAddon(search);
		t.loadAddon(new Unicode11Addon());
		t.unicode.activeVersion = "11";
		t.loadAddon(new WebLinksAddon((ev, url) => onLink(url, ev.metaKey)));
		if (visible) loadWebgl(t);
		t.open(el);
		// `el` is laid out: fit now, not after the attach answers (that painted the first frames at 80x24, then reflowed).
		if (el.offsetWidth && el.offsetHeight) addon.fit();
		const firstRender = t.onRender(() => {
			firstRender.dispose();
			drawn = true;
		});
		t.attachCustomKeyEventHandler((e) => {
			if (e.type !== "keydown") return true;
			if (e.metaKey && e.key === "f") {
				searchOpen = true;
				return false;
			}
			// Let app shortcuts (⌘T, ⌘W, ⌘\ …) reach the window handler.
			if (e.metaKey) return false;
			if (e.shiftKey && e.key === "Enter") {
				void host.ptyWrite(pty, "\x1b[13;2u");
				return false;
			}
			return true;
		});
		const offs: (() => void)[] = [];
		let alive = true;
		const pendingLive: Uint8Array[] = [];
		let replayed = false;
		listen<{ data: string; replay: boolean }>(`pty://${pty}`, (e) => {
			const bytes = b64bytes(e.payload.data);
			if (e.payload.replay) {
				// Fresh buffer behind a hidden host: the reset's blank frame is never on screen.
				drawn = false;
				t.reset();
				t.write(bytes, () => {
					const render = t.onRender(() => {
						render.dispose();
						drawn = true;
					});
					t.refresh(0, t.rows - 1);
				});
				replayed = true;
				pendingLive.splice(0).forEach((b) => t.write(b));
			} else if (replayed) t.write(bytes);
			else pendingLive.push(bytes);
		}).then((u) => (alive ? offs.push(u) : u()));
		const attach = () =>
			host
				.ptyAttach(pty, true)
				.then(() => {
					fit?.fit();
					return host.ptyResize(pty, t.cols, t.rows);
				})
				.catch(() => {});
		void attach();
		listen("host://reconnected", () => {
			replayed = false;
			void attach();
		}).then((u) => (alive ? offs.push(u) : u()));
		if (!readonly) t.onData((d) => void host.ptyWrite(pty, d).catch(() => {}));
		let timer: number | undefined;
		const ro = new ResizeObserver(() => {
			clearTimeout(timer);
			timer = window.setTimeout(() => {
				if (!el.offsetWidth || !el.offsetHeight) return;
				addon.fit();
				void host.ptyResize(pty, t.cols, t.rows).catch(() => {});
			}, 50);
		});
		ro.observe(el);
		return () => {
			alive = false;
			ro.disconnect();
			offs.forEach((u) => u());
			void host.ptyDetach(pty).catch(() => {});
			t.dispose();
		};
	});

	function find(dir: 1 | -1) {
		if (!query) return;
		if (dir > 0) search?.findNext(query);
		else search?.findPrevious(query);
	}
</script>

<div class="wrap">
	{#if searchOpen}
		<div class="search">
			<!-- svelte-ignore a11y_autofocus -->
			<FindInput
				autofocus
				placeholder="Find"
				bind:value={query}
				oninput={() => find(1)}
				onkeydown={(e) => {
					if (e.key === "Enter") find(e.shiftKey ? -1 : 1);
					if (e.key === "Escape") {
						searchOpen = false;
						search?.clearDecorations();
						term?.focus();
					}
				}}
			/>
		</div>
	{/if}
	<div
		class="xterm-host"
		bind:this={el}
		style:background={hostBg}
		style:visibility={drawn ? undefined : "hidden"}
		style:padding="{prefs.value.terminal.padding_y}px {prefs.value.terminal.padding_x}px"
	></div>
</div>

<style>
	.wrap {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
		background: var(--ca-surface-base);
	}
	.xterm-host {
		flex: 1;
		min-width: 0;
		min-height: 0;
	}
	.search {
		position: absolute;
		top: 6px;
		right: 14px;
		z-index: 5;
	}
</style>
