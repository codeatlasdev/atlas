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
	let search: SearchAddon | undefined;
	let searchOpen = $state(false);
	let query = $state("");

	/** Theme colors over the surface/text vars (WebGL can't draw a transparent canvas, so bg is always real). */
	function xtermColors() {
		const css = getComputedStyle(el);
		const bg = css.getPropertyValue("--ca-surface-base").trim() || css.backgroundColor;
		const fg = css.getPropertyValue("--ca-text-primary").trim() || css.color;
		const themed = Object.fromEntries(Object.entries(themes.xterm).filter(([, v]) => v !== undefined));
		return { background: bg, foreground: fg, cursor: fg, ...themed };
	}

	$effect(() => {
		if (focused && term) term.focus();
	});

	// A theme switch recolors open terminals; reads `themes.current` so the effect re-runs.
	$effect(() => {
		void themes.current;
		if (term) term.options.theme = xtermColors();
	});

	onMount(() => {
		const t = new Terminal({
			allowProposedApi: true,
			scrollback: prefs.value.terminal.max_scroll_history_lines,
			fontFamily: prefs.value.terminal.font_family,
			fontSize: prefs.value.terminal.font_size,
			cursorBlink: prefs.value.terminal.blinking,
			cursorStyle: ({ block: "block", bar: "bar", underline: "underline", hollow: "block" } as const)[
				prefs.value.terminal.cursor_shape
			],
			disableStdin: readonly,
			macOptionIsMeta: true,
			theme: xtermColors(),
		});
		term = t;
		const fit = new FitAddon();
		search = new SearchAddon();
		t.loadAddon(fit);
		t.loadAddon(search);
		t.loadAddon(new Unicode11Addon());
		t.unicode.activeVersion = "11";
		t.loadAddon(new WebLinksAddon((ev, url) => (onLink ? onLink(url, ev.metaKey) : void api.openUrl(url))));
		t.open(el);
		try {
			t.loadAddon(new WebglAddon());
		} catch {
			// canvas renderer fallback
		}
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
				t.reset();
				t.write(bytes);
				replayed = true;
				pendingLive.splice(0).forEach((b) => t.write(b));
			} else if (replayed) t.write(bytes);
			else pendingLive.push(bytes);
		}).then((u) => (alive ? offs.push(u) : u()));
		const attach = () =>
			host
				.ptyAttach(pty, true)
				.then(() => {
					fit.fit();
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
				fit.fit();
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
			<input
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
	<div class="xterm-host" bind:this={el}></div>
</div>

<style>
	.wrap {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
		padding: 6px 0 0 10px;
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
	.search input {
		font: inherit;
		font-size: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-bg-default));
		color: var(--ca-text-primary);
		width: 200px;
	}
</style>
