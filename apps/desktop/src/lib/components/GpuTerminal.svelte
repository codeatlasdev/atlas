<script lang="ts">
	import { onMount } from "svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { toast } from "@codeatlasdev/ui";
	import { host } from "#lib/host.ts";
	import { errText, api } from "#lib/api.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { gpu } from "#lib/stores/gpu.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { termFocus } from "#lib/stores/termFocus.svelte.ts";
	import { runCommand } from "#lib/keymap.svelte.ts";
	import { fontStep, optionActsAsAlt, termAction, type TermAction } from "#lib/termKeys.ts";

	let {
		tab,
		pty,
		visible,
		focused = false,
		readonly = false,
		onLink,
	}: {
		tab: string;
		pty: string;
		visible: boolean;
		focused?: boolean;
		/** Ended terminal: no keys, paste or drops reach the pty. */
		readonly?: boolean;
		onLink?: (url: string, external: boolean) => void;
	} = $props();

	let area: HTMLDivElement | undefined = $state();
	let input: HTMLTextAreaElement | undefined = $state();
	let searchInput: HTMLInputElement | undefined = $state();
	let opened = false;
	let failed = false;

	let searchOpen = $state(false);
	let query = $state("");
	let useRegex = $state(false);
	let matchCase = $state(false);
	let stat = $state<{ index: number; total: number } | null>(null);
	let searchError = $state(false);

	function focusInput() {
		if (searchOpen) searchInput?.focus();
		else input?.focus();
	}

	$effect(() => {
		if (focused && visible) focusInput();
	});

	/** Theme color as #rrggbb (the kit uses oklch; a canvas normalizes any CSS color). */
	function hex(cssVar: string, fallback: string): string {
		const v = getComputedStyle(area!).getPropertyValue(cssVar).trim() || fallback;
		const ctx = document.createElement("canvas").getContext("2d")!;
		ctx.fillStyle = v;
		ctx.fillRect(0, 0, 1, 1);
		const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
		return "#" + [r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("");
	}

	// Observers fire concurrently; serialize so `term_open` runs once before any `term_bounds`.
	let chain = Promise.resolve();
	function sync() {
		chain = chain.then(apply);
		return chain;
	}

	async function apply() {
		if (!area || failed) return;
		const r = area.getBoundingClientRect();
		const rect = { x: r.left, y: r.top, w: r.width, h: r.height };
		// The native view sits above the webview: park it under dialogs/palette.
		const show =
			visible && r.width > 0 && r.height > 0 && !document.querySelector("[role=dialog][data-state=open]");
		try {
			if (!opened) {
				if (!show) return;
				opened = true;
				await invoke("term_open", { tab, pty, ...rect, ...colors(), prefs: termPrefs() });
			}
			await invoke("term_bounds", { tab, ...rect, visible: show });
		} catch (e) {
			failed = true;
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

	$effect(() => {
		void visible;
		void sync();
	});

	function colors() {
		return {
			fg: themes.terminal?.fg ?? hex("--ca-text-primary", "#e6e6e6"),
			bg: themes.terminal?.bg ?? hex("--ca-surface-base", "#111111"),
			ansi16: themes.terminal?.ansi16,
			min_contrast: prefs.value.terminal.minimum_contrast,
		};
	}

	function termPrefs() {
		const t = prefs.value.terminal;
		return {
			font_size: t.font_size,
			scrollback: t.max_scroll_history_lines,
			cursor_shape: t.cursor_shape,
			cursor_blink: t.blinking,
			osc52: t.osc52,
			ligatures: t.ligatures,
		};
	}

	// A theme switch recolors this terminal live (reads `themes.current` so it re-runs).
	$effect(() => {
		void themes.current;
		const c = themes.terminal;
		if (!opened || !area) return;
		void c;
		void invoke("term_recolor", { tab, ...colors() }).catch(() => {});
	});

	// `terminal.*` settings apply live: scrollback, cursor, OSC 52.
	$effect(() => {
		const p = termPrefs();
		if (opened) void invoke("term_set_prefs", { tab, prefs: p }).catch(() => {});
	});

	const NO_INPUT = new Set([
		"Shift",
		"Control",
		"Alt",
		"Meta",
		"CapsLock",
		"Dead",
		"Process",
		"Unidentified",
	]);

	/** Which ⌥ key is down (a chord's own event does not say), for `terminal.option_as_alt`. */
	let optionSide: "left" | "right" | null = null;

	function onkeyup(e: KeyboardEvent) {
		if (e.key === "Meta") clearHover();
		if (e.code === "AltLeft" || e.code === "AltRight") optionSide = null;
	}

	/** Font size this terminal is at (⌘+ / ⌘− step it; a `terminal.font_size` change resets it). */
	let fontNow = prefs.value.terminal.font_size;
	$effect(() => {
		fontNow = prefs.value.terminal.font_size;
	});

	function runTermAction(a: TermAction) {
		const quiet = () => {};
		switch (a.kind) {
			case "search":
				return openSearch();
			case "copy":
				return void invoke("term_copy", { tab }).catch(quiet);
			case "selectAll":
				return void invoke("term_select_all", { tab }).catch(quiet);
			case "jumpPrompt":
				return void invoke("term_jump_prompt", { tab, up: a.up }).catch(quiet);
			case "scroll":
				return void invoke("term_scroll_to", { tab, to: a.to }).catch(quiet);
			case "split":
				return runCommand(a.dir === "right" ? "pane::SplitRight" : "pane::SplitDown");
			case "font": {
				const next = fontStep(fontNow, prefs.value.terminal.font_size, a.change);
				fontNow = next;
				return void invoke("term_font", { tab, size: next }).catch(quiet);
			}
			case "clear":
				return void (!readonly && invoke("term_clear", { tab }).catch(quiet));
			case "key":
				if (!readonly)
					void invoke("term_key", {
						tab,
						key: a.name,
						shift: false,
						alt: false,
						ctrl: false,
						meta: a.meta,
					}).catch(quiet);
		}
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.code === "AltLeft") optionSide = "left";
		else if (e.code === "AltRight") optionSide = "right";
		// ⌘ chords: Ghostty's terminal actions and the macOS line-editing keys. Other ⌘
		// chords belong to the app (menu / keymap) or arrive as paste events.
		const action = termAction(e);
		if (action) {
			e.preventDefault();
			return runTermAction(action);
		}
		if (e.metaKey || e.isComposing || NO_INPUT.has(e.key)) return;
		if (readonly) return void e.preventDefault();
		let key = e.key;
		const alt = e.altKey && optionActsAsAlt(prefs.value.terminal.option_as_alt, optionSide, key);
		// Option as Meta: the physical key, not the macOS option-layer character.
		if (alt && /^(Key[A-Z]|Digit\d)$/.test(e.code)) {
			const c = e.code.slice(-1);
			key = e.shiftKey ? c : c.toLowerCase();
		}
		// Plain printable keys come through `input` (IME and dead keys composed there); so does
		// ⌥-typed text when ⌥ is not Alt.
		if (key.length === 1 && !e.ctrlKey && !alt) return;
		e.preventDefault();
		void invoke("term_key", { tab, key, shift: e.shiftKey, alt, ctrl: e.ctrlKey }).catch(() => {});
	}

	function oninput(e: Event) {
		const ie = e as InputEvent;
		if (ie.isComposing) return;
		const text = input!.value;
		input!.value = "";
		if (text && !readonly)
			void invoke("term_key", { tab, key: "", text, shift: false, alt: false, ctrl: false }).catch(() => {});
	}

	function oncompositionend() {
		oninput(new InputEvent("input"));
	}

	/**
	 * Edit ▸ Copy (⌘C is a menu key equivalent: AppKit sends `copy:`, WebKit fires this event on
	 * the focused sink and the keydown never reaches the page). The selection lives in the
	 * renderer, not the DOM, so this is the one path that copies it on ⌘C.
	 */
	function oncopy(e: ClipboardEvent) {
		e.preventDefault();
		void invoke("term_copy", { tab }).catch(() => {});
	}

	/** Edit ▸ Paste: one `paste` event per ⌘V (the keydown is consumed by the menu). */
	function onpaste(e: ClipboardEvent) {
		e.preventDefault();
		const text = e.clipboardData?.getData("text/plain");
		if (text && !readonly) void invoke("term_paste", { tab, text }).catch(() => {});
	}

	/** `'path'` with embedded quotes escaped, as the xterm tab's drop does. */
	const shellQuote = (p: string) => `'${p.replace(/'/g, "'\\''")}' `;

	function ondragover(e: DragEvent) {
		if (!readonly && e.dataTransfer?.types.includes("text/plain")) e.preventDefault();
	}

	function ondrop(e: DragEvent) {
		const path = e.dataTransfer?.getData("text/plain");
		if (readonly || !path || e.dataTransfer?.types.includes("atlas/tab")) return;
		e.preventDefault();
		e.stopPropagation();
		void invoke("term_paste", { tab, text: shellQuote(path) }).catch(() => {});
	}

	// ---- search (⌘F) ----

	function openSearch() {
		searchOpen = true;
		queueMicrotask(() => {
			searchInput?.focus();
			searchInput?.select();
		});
	}

	function closeSearch() {
		searchOpen = false;
		stat = null;
		searchError = false;
		void invoke("term_search_clear", { tab }).catch(() => {});
		input?.focus();
	}

	async function find(forward: boolean) {
		if (!query) {
			stat = null;
			searchError = false;
			return void invoke("term_search_clear", { tab }).catch(() => {});
		}
		try {
			stat = await invoke<{ index: number; total: number }>("term_search", {
				tab,
				query,
				regex: useRegex,
				case: matchCase,
				forward,
			});
			searchError = false;
		} catch {
			// Invalid regex while typing: keep the last highlights, flag the field.
			searchError = true;
		}
	}

	function onsearchkey(e: KeyboardEvent) {
		e.stopPropagation();
		if (e.key === "Enter") {
			e.preventDefault();
			void find(!e.shiftKey);
		} else if (e.key === "Escape") {
			e.preventDefault();
			closeSearch();
		} else if (e.metaKey && e.key === "f") {
			e.preventDefault();
			searchInput?.select();
		}
	}

	/** Exact cell size (logical px) reported by the renderer; refreshed per press (resizes change it). */
	let cell: { w: number; h: number } | null = null;
	let selecting = false;

	async function fetchCell() {
		const s = await invoke<Record<string, { cell_w: number; cell_h: number }>>("term_stats");
		const t = s[tab];
		cell = t && t.cell_w > 0 && t.cell_h > 0 ? { w: t.cell_w, h: t.cell_h } : null;
	}

	/** Viewport cell under the pointer, plus the x fraction inside it (selection side). */
	function toCell(e: MouseEvent) {
		if (!area || !cell) return null;
		const r = area.getBoundingClientRect();
		const fx = (e.clientX - r.left) / cell.w;
		const fy = (e.clientY - r.top) / cell.h;
		return {
			col: Math.max(0, Math.floor(fx)),
			row: Math.max(0, Math.floor(fy)),
			frac: fx - Math.floor(fx),
		};
	}

	function openLink(url: string, external: boolean) {
		if (onLink) onLink(url, external);
		else void api.openUrl(url).catch((err) => toast.error(errText(err)));
	}

	async function onmousedown(e: MouseEvent) {
		if (e.button !== 0) return;
		// Keep the textarea focused for keys/paste (the mousedown default would blur it).
		e.preventDefault();
		input?.focus();
		await fetchCell().catch(() => {});
		const c = toCell(e);
		if (!c) return;
		if (e.metaKey) {
			const url = await invoke<string | null>("term_hyperlink_at", { tab, col: c.col, row: c.row }).catch(
				() => null,
			);
			if (url) {
				hovered = false;
				void invoke("term_hover", { tab }).catch(() => {});
				openLink(url, true);
				return;
			}
		}
		selecting = true;
		// A plain single click may be a click-to-move; a drag or a multi-click is a selection.
		press = e.detail === 1 && !e.altKey && !e.shiftKey && !e.metaKey ? { col: c.col, row: c.row } : null;
		void invoke("term_select_start", {
			tab,
			col: c.col,
			row: c.row,
			frac: c.frac,
			clicks: Math.min(e.detail, 3),
			block: e.altKey,
		}).catch(() => {});
	}

	/** Cell of an in-progress plain click; cleared once the pointer leaves it. */
	let press: { col: number; row: number } | null = null;

	/** A URL underline is showing (cleared when ⌘ is released or the pointer leaves). */
	let hovered = false;
	let lastHover = "";

	function hoverUrl(e: MouseEvent) {
		if (!e.metaKey) return clearHover();
		const c = toCell(e);
		if (!c) return;
		const key = `${c.col},${c.row}`;
		if (key === lastHover) return;
		lastHover = key;
		hovered = true;
		void invoke("term_hover", { tab, col: c.col, row: c.row }).catch(() => {});
	}

	function clearHover() {
		lastHover = "";
		if (!hovered) return;
		hovered = false;
		void invoke("term_hover", { tab }).catch(() => {});
	}

	function onmousemove(e: MouseEvent) {
		if (!selecting) return hoverUrl(e);
		if (!(e.buttons & 1)) return void onmouseup(e);
		const c = toCell(e);
		if (c && press && (c.col !== press.col || c.row !== press.row)) press = null;
		if (c) void invoke("term_select_update", { tab, col: c.col, row: c.row, frac: c.frac }).catch(() => {});
	}

	async function onmouseup(e: MouseEvent) {
		if (!selecting) return;
		selecting = false;
		const c = toCell(e);
		const click = press && c && c.col === press.col && c.row === press.row ? press : null;
		press = null;
		if (c) void invoke("term_select_update", { tab, col: c.col, row: c.row, frac: c.frac }).catch(() => {});
		const t = prefs.value.terminal;
		// Ghostty `cursor-click-to-move`: a click on the prompt line moves the shell's cursor there.
		if (click && t.click_to_move && !readonly) {
			const moved = await invoke<boolean>("term_click_move", { tab, col: click.col, row: click.row }).catch(
				() => false,
			);
			if (moved) return void invoke("term_clear_selection", { tab }).catch(() => {});
		}
		void invoke("term_select_end", { tab, copy: t.copy_on_select }).catch(() => {});
	}

	function onwheel(e: WheelEvent) {
		e.preventDefault();
		const lines = Math.round(-e.deltaY / (cell?.h ?? 16));
		if (lines) void invoke("term_scroll", { tab, delta: lines }).catch(() => {});
	}

	onMount(() => {
		const focus = listen<string>("term://focus", (e) => {
			if (e.payload === tab) input?.focus();
		});
		// OSC 7: Restart reopens in the shell's current directory.
		// OSC 52 with `terminal.osc52: "ask"`: the program wants to set the clipboard.
		const osc52 = listen<{ tab: string; text: string }>("term://osc52", async (e) => {
			if (e.payload.tab !== tab) return;
			const t = e.payload.text;
			const preview = t.length > 80 ? `${t.slice(0, 80)}…` : t;
			const ok = await confirmDialog("Copy to clipboard?", {
				description: `A program in this terminal wants to copy ${t.length} characters to the clipboard:\n\n${preview}`,
				confirmLabel: "Allow",
			});
			if (ok) void navigator.clipboard.writeText(t);
		});
		const cwd = listen<{ tab: string; cwd: string }>("term://cwd", (e) => {
			const t = e.payload.tab === tab ? ws.tab(tab) : undefined;
			if (t) t.cwd = e.payload.cwd;
		});
		const ro = new ResizeObserver(() => void sync());
		ro.observe(area!);
		const mo = new MutationObserver(() => void sync());
		mo.observe(document.body, {
			subtree: true,
			attributes: true,
			attributeFilter: ["data-state"],
			childList: true,
		});
		window.addEventListener("resize", sync);
		// Title/exit events for the tab; bytes come over the raw stream.
		void host.ptyAttach(pty, false, false).catch(() => {});
		return () => {
			void focus.then((f) => f());
			void cwd.then((f) => f());
			void osc52.then((f) => f());
			ro.disconnect();
			mo.disconnect();
			window.removeEventListener("resize", sync);
			if (document.activeElement === input) termFocus.on = false;
			void host.ptyDetach(pty).catch(() => {});
			void invoke("term_close", { tab }).catch(() => {});
		};
	});
</script>

<div class="wrap">
	<!-- The native terminal view paints above the webview, so the bar takes its own row
	     (the terminal shrinks by its height) instead of floating over the text. -->
	{#if searchOpen}
		<div class="search" role="search">
			<input
				bind:this={searchInput}
				class:bad={searchError}
				placeholder="Find"
				aria-label="Find in terminal"
				spellcheck="false"
				autocomplete="off"
				bind:value={query}
				oninput={() => void find(true)}
				onkeydown={onsearchkey}
			/>
			<button
				class="toggle"
				class:on={matchCase}
				title="Match case"
				aria-label="Match case"
				aria-pressed={matchCase}
				onclick={() => {
					matchCase = !matchCase;
					void find(true);
				}}>Aa</button
			>
			<button
				class="toggle"
				class:on={useRegex}
				title="Regular expression"
				aria-label="Regular expression"
				aria-pressed={useRegex}
				onclick={() => {
					useRegex = !useRegex;
					void find(true);
				}}>.*</button
			>
			<span class="count" aria-live="polite">
				{#if searchError}invalid{:else if stat && query}{stat.total
						? `${stat.index}/${stat.total}`
						: "0"}{/if}
			</span>
		</div>
	{/if}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		class="area"
		bind:this={area}
		{onmousedown}
		{onmousemove}
		{onmouseup}
		{onwheel}
		{ondragover}
		{ondrop}
		onmouseleave={clearHover}
	>
		<textarea
			bind:this={input}
			class="sink"
			data-term-tab={tab}
			aria-label="Terminal input"
			autocomplete="off"
			autocapitalize="off"
			spellcheck="false"
			{onkeydown}
			{onkeyup}
			{oninput}
			{oncompositionend}
			onfocus={() => {
				termFocus.on = true;
				void invoke("term_focus", { tab, focused: true }).catch(() => {});
			}}
			onblur={() => {
				termFocus.on = false;
				void invoke("term_focus", { tab, focused: false }).catch(() => {});
			}}
			{oncopy}
			{onpaste}></textarea>
	</div>
</div>

<style>
	.wrap {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	/* Same inset as the xterm host. */
	.area {
		position: relative;
		flex: 1;
		min-height: 0;
		margin: 6px 0 0 10px;
	}
	/* Focus target for keys/IME/paste; the native view draws the terminal. */
	.sink {
		position: absolute;
		opacity: 0;
		width: 1px;
		height: 1px;
		padding: 0;
		border: 0;
		resize: none;
		pointer-events: none;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 4px;
		justify-content: flex-end;
		padding: 6px 14px 0 10px;
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
	.search input.bad {
		border-color: var(--ca-semantic-danger, #e5484d);
	}
	.toggle {
		font: inherit;
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary, var(--ca-text-primary));
		cursor: pointer;
	}
	.toggle.on {
		background: var(--ca-surface-raised, var(--ca-bg-default));
		color: var(--ca-text-primary);
		border-color: var(--ca-text-secondary, var(--ca-text-primary));
	}
	.count {
		min-width: 36px;
		font-size: 11px;
		color: var(--ca-text-secondary, var(--ca-text-primary));
		font-variant-numeric: tabular-nums;
	}
</style>
