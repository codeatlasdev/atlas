<script lang="ts">
	import { onMount } from "svelte";
	import { Terminal } from "@xterm/xterm";
	import { FitAddon } from "@xterm/addon-fit";
	import { SearchAddon } from "@xterm/addon-search";
	import { WebLinksAddon } from "@xterm/addon-web-links";
	import { WebglAddon } from "@xterm/addon-webgl";
	import { Unicode11Addon } from "@xterm/addon-unicode11";
	import { listen } from "@tauri-apps/api/event";
	import { host, b64bytes } from "#lib/host.ts";
	import { api } from "#lib/api.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import FindInput from "./FindInput.svelte";
	import { IS_MAC, primaryKey } from "#lib/keys.ts";
	import { xtermKey } from "#lib/termKeys.ts";
	import { ptyStream, type PtyEvent } from "#lib/ptyStream.ts";

	let {
		pty,
		focused = false,
		readonly = false,
		onLink,
	}: {
		pty: string;
		focused?: boolean;
		readonly?: boolean;
		onLink?: (url: string, external: boolean) => void;
	} = $props();

	let el: HTMLDivElement;
	let term: Terminal | undefined;
	let fit: FitAddon | undefined;
	let search: SearchAddon | undefined;
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
		t.loadAddon(
			new WebLinksAddon((ev, url) => (onLink ? onLink(url, primaryKey(ev)) : void api.openUrl(url))),
		);
		t.open(el);
		try {
			t.loadAddon(new WebglAddon());
		} catch {
			// canvas renderer fallback
		}
		t.attachCustomKeyEventHandler((e) => {
			if (e.type !== "keydown") return true;
			switch (xtermKey(e, t.hasSelection(), IS_MAC)) {
				case "search":
					searchOpen = true;
					return false;
				case "copy":
					void navigator.clipboard.writeText(t.getSelection());
					t.clearSelection();
					e.preventDefault();
					return false;
				// The browser's own paste follows; xterm's paste handler sends the text.
				case "paste":
				// App shortcuts (⌘T, ⌘W, ⌘\ …; Ctrl+Shift+P, Ctrl+Tab … off macOS) reach the window handler.
				case "app":
					return false;
				case "shiftEnter":
					void host.ptyWrite(pty, "\x1b[13;2u");
					return false;
			}
			return true;
		});
		const offs: (() => void)[] = [];
		let alive = true;
		const stream = ptyStream({
			listen: (f) => listen<PtyEvent>(`pty://${pty}`, (e) => f(e.payload)),
			attach: () =>
				host.ptyAttach(pty, true).then(() => {
					fit?.fit();
					return host.ptyResize(pty, t.cols, t.rows);
				}),
			decode: b64bytes,
			write: (b) => t.write(b),
			reset: () => t.reset(),
		});
		offs.push(() => stream.dispose());
		listen("host://reconnected", () => stream.reattach()).then((u) => (alive ? offs.push(u) : u()));
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
			clearTimeout(timer);
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
