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
	import { isSwiping, watchSlot } from "./nativeSlot.ts";
	import { shellQuote } from "./terminal/geometry.ts";
	import { closeHole, openHole, recut } from "./terminal/holeLayers.ts";
	import { contentRect } from "./terminal/fonts.ts";
	import { NativeView, listenFontFallback, termPrefs } from "./terminal/nativeView.ts";
	import { registerTermView } from "#lib/stores/termViews.ts";
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
	/**
	 * The native view is on screen under the slot (the backend says so: `term://presented`, and the reply of every
	 * layout batch). Only then is the slot a transparent hole; until then, and for a tab that is not on screen,
	 * the slot paints the terminal colour. A pane the tab pager moves by a CSS transform (`data-swiping`) also
	 * paints it: the view cannot follow the transform, and the opaque slot covers it while it moves.
	 */
	let presented = $state(false);
	// A pane in flight (`data-swiping`), and the beat after it lands: the mark lifts in the DOM at once, but the native
	// view only reaches the pane's rest rect with the next layout batch, so the slot keeps painting until that answers.
	let swiping = $state(false);
	let settling = $state(false);
	const moving = $derived(swiping || settling);
	const view = new NativeView(
		() => ({ tab, pty }),
		() => area,
		() => visible,
		(p) => {
			presented = p;
			settling = false;
			// Every layout answer: the rect may have moved with `presented` unchanged (a split dragged).
			recut();
		},
	);
	// ⌘ held over the terminal: links are live, so the slot shows the pointer cursor (the page owns the cursor now).
	let linkMode = $state(false);
	const pointer = new Pointer({
		onLinkMode: (on) => (linkMode = on),
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
	// The hole: transparent while the native view is really under the slot and still. The page's backdrop layers (ground,
	// card) are cut at the slot's rect while it is open (`holeLayers.ts`), so the terminal shows through them too.
	const hole = $derived(presented && visible && !moving);
	$effect(() => {
		if (hole && area) openHole(tab, () => (area ? contentRect(area) : null));
		else closeHole(tab);
	});

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
		// A pane in flight carries `data-swiping` (tabPager.ts): the slot paints while it moves.
		const unwatch = watchSlot(area!, view.sync, () => {
			const now = isSwiping(area);
			if (swiping && !now) settling = true;
			swiping = now;
		});
		const unregister = registerTermView(tab, view);
		// Title/exit events for the tab; bytes come over the raw stream.
		void host.ptyAttach(pty, false, false).catch(quiet);
		return () => {
			void focus.then((f) => f());
			void cwd.then((f) => f());
			void osc52.then((f) => f());
			unwatch();
			closeHole(tab);
			view.dispose();
			unregister();
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
		style:background={hole ? "transparent" : slotBg}
		style:border-color={slotBg}
		style:border-width="{prefs.value.terminal.padding_y}px {prefs.value.terminal.padding_x}px"
		class="area"
		class:hole
		class:link={linkMode}
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
	/* The slot fills the pane. Its border is the padding band (`terminal.padding_x/y`), always painted in the terminal
	   colour (Ghostty `window-padding-color = extend`); its content box is exactly the native view's rect and, while the
	   view is presented, a transparent hole. */
	.area {
		position: relative;
		flex: 1;
		min-height: 0;
		box-sizing: border-box;
		border-style: solid;
		/* The page sits above the native view, so the mouse cursor is the page's: the terminal's is the text I-beam. */
		cursor: text;
	}
	.area.link {
		cursor: pointer;
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
