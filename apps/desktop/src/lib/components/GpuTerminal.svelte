<script lang="ts">
	/**
	 * Native GPU terminal tab: a DOM slot the native view is laid over, plus a hidden textarea
	 * that receives keys, IME and paste. Geometry, mouse and view sync live in `terminal/`.
	 */
	import { onMount } from "svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { toast } from "@codeatlasdev/ui";
	import { host } from "#lib/host.ts";
	import { errText, api } from "#lib/api.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { termFocus } from "#lib/stores/termFocus.svelte.ts";
	import { runCommand } from "#lib/keymap.svelte.ts";
	import { fontStep, keyToSend, producesInput, termAction, type TermAction } from "#lib/termKeys.ts";
	import { watchSlot } from "./nativeSlot.ts";
	import { shellQuote } from "./terminal/geometry.ts";
	import { NativeView, listenFontFallback, termPrefs } from "./terminal/nativeView.ts";
	import { Pointer } from "./terminal/pointer.ts";
	import TermSearchBar from "./terminal/TermSearchBar.svelte";

	let {
		tab,
		pty,
		visible,
		focused = false,
		readonly = false,
		onLink,
		onFile,
	}: {
		tab: string;
		pty: string;
		visible: boolean;
		focused?: boolean;
		/** Ended terminal: no keys, paste or drops reach the pty. */
		readonly?: boolean;
		onLink?: (url: string, external: boolean) => void;
		/** ⌘-click on a plain token: `true` when it named a file and was opened. */
		onFile?: (token: string) => boolean;
	} = $props();

	let area: HTMLDivElement | undefined = $state();
	let input: HTMLTextAreaElement | undefined = $state();
	let searchBar: TermSearchBar | undefined = $state();

	let searchOpen = $state(false);
	let query = $state("");
	let useRegex = $state(false);
	let matchCase = $state(false);

	const quiet = () => {};
	const view = new NativeView(
		() => ({ tab, pty }),
		() => area,
		() => visible,
	);
	const pointer = new Pointer({
		tab: () => tab,
		area: () => area,
		focusInput: () => input?.focus(),
		readonly: () => readonly,
		prefs: () => prefs.value.terminal,
		onLink: (url, external) => {
			if (onLink) onLink(url, external);
			else void api.openUrl(url).catch((err) => toast.error(errText(err)));
		},
		onFile: (token) => onFile?.(token) ?? false,
	});

	$effect(() => {
		if (focused && visible) {
			if (searchOpen) searchBar?.focus();
			else input?.focus();
		}
	});

	$effect(() => {
		void visible;
		void view.sync();
	});

	// A theme switch recolors this terminal live (reads `themes.current` so it re-runs).
	$effect(() => {
		void themes.current;
		view.recolor();
	});

	// The slot is painted with the terminal background from its very first paint (Ghostty `window-padding-color =
	// extend`; the same fallback as `termBg`, which needs the mounted element and so came a frame late). The native
	// view stays parked until its first frame is drawn (`Placement` in term/shared.rs), and the slot is all that shows until then:
	// it must be the terminal's own colour, not the webview behind it.
	const slotBg = $derived(themes.terminal?.bg ?? "var(--ca-surface-base)");

	// Padding moves the native rect: re-sync when it changes.
	$effect(() => {
		void prefs.value.terminal.padding_x;
		void prefs.value.terminal.padding_y;
		void view.sync();
	});

	// `terminal.*` settings apply live: scrollback, cursor, OSC 52.
	$effect(() => {
		void termPrefs();
		view.setPrefs();
	});

	/** Which ⌥ key is down (a chord's own event does not say), for `terminal.option_as_alt`. */
	let optionSide: "left" | "right" | null = null;

	function onkeyup(e: KeyboardEvent) {
		if (e.key === "Meta") pointer.clearHover();
		if (e.code === "AltLeft" || e.code === "AltRight") optionSide = null;
	}

	/** Font size this terminal is at (⌘+ / ⌘− step it; a `terminal.font_size` change resets it). */
	let fontNow = prefs.value.terminal.font_size;
	$effect(() => {
		fontNow = prefs.value.terminal.font_size;
	});

	function runTermAction(a: TermAction) {
		switch (a.kind) {
			case "search":
				return openSearch();
			case "searchNext":
				return searchOpen ? searchBar?.next(a.forward) : openSearch();
			case "selectOutput":
				return void invoke("term_select_output", { tab }).catch(quiet);
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
		if (readonly) return void (producesInput(e) && e.preventDefault());
		const send = keyToSend(e, prefs.value.terminal.option_as_alt, optionSide);
		if (!send) return;
		e.preventDefault();
		void invoke("term_key", { tab, ...send }).catch(quiet);
	}

	function oninput(e: Event) {
		if ((e as InputEvent).isComposing) return;
		const text = input!.value;
		input!.value = "";
		if (text && !readonly)
			void invoke("term_key", { tab, key: "", text, shift: false, alt: false, ctrl: false }).catch(quiet);
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
		void invoke("term_copy", { tab }).catch(quiet);
	}

	/**
	 * Edit ▸ Paste: one `paste` event per ⌘V (the keydown is consumed by the menu). The event's
	 * clipboardData only has text, so a screenshot or a Finder file copy would arrive empty:
	 * the host reads the pasteboard itself (text, quoted file paths, or the image hand-off).
	 */
	function onpaste(e: ClipboardEvent) {
		e.preventDefault();
		if (!readonly) void invoke("term_paste_clipboard", { tab }).catch(quiet);
	}

	function ondragover(e: DragEvent) {
		if (!readonly && e.dataTransfer?.types.includes("text/plain")) e.preventDefault();
	}

	function ondrop(e: DragEvent) {
		const path = e.dataTransfer?.getData("text/plain");
		if (readonly || !path || e.dataTransfer?.types.includes("atlas/tab")) return;
		e.preventDefault();
		e.stopPropagation();
		void invoke("term_paste", { tab, text: shellQuote(path) }).catch(quiet);
	}

	function openSearch() {
		searchOpen = true;
		queueMicrotask(() => searchBar?.focus());
	}

	function closeSearch() {
		searchOpen = false;
		input?.focus();
	}

	onMount(() => {
		listenFontFallback();
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
		const unwatch = watchSlot(area!, view.sync);
		// Title/exit events for the tab; bytes come over the raw stream.
		void host.ptyAttach(pty, false, false).catch(quiet);
		return () => {
			void focus.then((f) => f());
			void cwd.then((f) => f());
			void osc52.then((f) => f());
			unwatch();
			if (document.activeElement === input) termFocus.on = false;
			void host.ptyDetach(pty).catch(quiet);
			void invoke("term_close", { tab }).catch(quiet);
		};
	});
</script>

<div class="wrap">
	{#if searchOpen}
		<TermSearchBar
			bind:this={searchBar}
			{tab}
			bind:query
			bind:useRegex
			bind:matchCase
			onclose={closeSearch}
		/>
	{/if}
	<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
	<div
		style:background={slotBg}
		class="area"
		bind:this={area}
		onmousedown={pointer.mousedown}
		onmousemove={pointer.mousemove}
		onmouseup={pointer.mouseup}
		onwheel={pointer.wheel}
		{ondragover}
		{ondrop}
		onmouseleave={pointer.clearHover}
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
				void invoke("term_focus", { tab, focused: true }).catch(quiet);
			}}
			onblur={() => {
				termFocus.on = false;
				void invoke("term_focus", { tab, focused: false }).catch(quiet);
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
	/* The slot fills the pane in the terminal background; `terminal.padding_x/y` insets the native view inside it. */
	.area {
		position: relative;
		flex: 1;
		min-height: 0;
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
</style>
