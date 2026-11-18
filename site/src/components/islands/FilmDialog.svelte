<script lang="ts">
	// "Play the film": one button in the hero; the full film (FilmPlayer, chapters, captions) opens in a modal <dialog>.
	// Desktop/tablet: the dialog plays inline. Portrait phone: the 16:9 film in a 360 px box is unwatchable, so the tap
	// hands the <video> to the native fullscreen player (iOS: webkitEnterFullscreen; it rotates, has its own controls)
	// and, where that is not available, the dialog says "rotate your phone" (see mobileFilmUi in data/film.ts).
	import { onMount, type Snippet } from "svelte";
	import FilmPlayer from "./FilmPlayer.svelte";
	import { mobileFilmUi } from "../../data/film";
	import "../../lib/locale-client";
	import { m } from "../../lib/paraglide/messages.js";

	let { poster, mp4, webm, captions, thumbs, clock, words, fallback }: { poster: string; mp4: string; webm: string; captions: string; thumbs: string; clock: string; words: string; fallback?: Snippet } = $props();

	let dlg: HTMLDialogElement;
	let player = $state<FilmPlayer>();
	let autostart = $state<{ nativeFullscreen: boolean }>();
	let mounted = $state(false);
	let open = $state(false);
	// The player (and with it the poster) mounts on first open: a visitor who never presses play pays nothing for it.
	let everOpened = $state(false);
	let hint = $state(false);
	let opener: HTMLElement | null = null;

	onMount(() => (mounted = true));

	async function show(e: MouseEvent) {
		opener = e.currentTarget as HTMLElement;
		const ui = mobileFilmUi({ portrait: matchMedia("(orientation: portrait)").matches, width: innerWidth, nativeFullscreen: !!(document.createElement("video") as HTMLVideoElement & { webkitEnterFullscreen?: unknown }).webkitEnterFullscreen || !!document.documentElement.requestFullscreen });
		hint = ui.rotateHint && !ui.fullscreenOnPlay;
		dlg.showModal();
		document.documentElement.classList.add("film-open");
		// First tap: mount the player with autostart (it plays from its own onMount, inside this gesture). Later taps: play the mounted one.
		if (!everOpened) autostart = { nativeFullscreen: ui.fullscreenOnPlay };
		open = everOpened = true;
		if (player) await player.play({ nativeFullscreen: ui.fullscreenOnPlay });
	}

	function closed() {
		open = false;
		hint = false;
		player?.stop();
		document.documentElement.classList.remove("film-open");
		opener?.focus();
	}

	function onBackdrop(e: MouseEvent) {
		if (e.target === dlg) dlg.close();
	}
</script>

<button type="button" class="playbtn" onclick={show} disabled={!mounted} aria-haspopup="dialog">
	<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 3.5v17l15-8.5z" /></svg>
	<b>{m.islands_film_dialog_play_the_film()}</b>
	<small aria-label={words}>{clock}</small>
	<span class="snd">{m.islands_film_dialog_with_sound()}</span>
</button>

<dialog bind:this={dlg} class="filmdlg" aria-label={m.islands_film_label({ duration: words })} onclose={closed} onclick={onBackdrop}>
	<div class="in">
		<button type="button" class="x" onclick={() => dlg.close()} aria-label={m.islands_film_dialog_close_the_film()}><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" aria-hidden="true"><path d="M5 5l14 14M19 5L5 19" /></svg></button>
		{#if everOpened}<FilmPlayer bind:this={player} {poster} {mp4} {webm} {captions} {thumbs} {words} {autostart} />{/if}
		{#if hint}<p class="rotate" role="status">{m.islands_film_dialog_rotate_hint()}</p>{/if}
	</div>
</dialog>

<style>
	.playbtn { justify-self: start; display: inline-flex; align-items: center; gap: 0.7rem; min-height: 48px; padding: 0.6rem 1rem 0.6rem 0.8rem; background: #fff; color: var(--ink); border: 0; border-radius: 3px; cursor: pointer; font: 800 1.05rem/1 var(--font); font-stretch: 80%; box-shadow: 4px 4px 0 var(--blue-deep); transition: background 0.12s, transform 0.12s; }
	.playbtn svg { color: var(--blue); }
	.playbtn small { font: 600 0.82rem/1 var(--mono); color: var(--ink-2); }
	.playbtn .snd { font: 600 0.82rem/1 var(--font); color: var(--ink-2); }
	.playbtn:hover, .playbtn:focus-visible { background: var(--wait); }
	.playbtn:active { transform: translateY(2px); }
	.playbtn:disabled { opacity: 0.7; cursor: progress; }
	.playbtn:focus-visible { outline: 3px solid var(--wait); outline-offset: 3px; }
	@media (max-width: 640px) { .playbtn { justify-self: stretch; justify-content: center; box-shadow: none; border: 2px solid #fff; background: transparent; color: #fff; min-height: 52px; } .playbtn svg { color: currentColor; } .playbtn small, .playbtn .snd { color: rgb(255 255 255 / 0.8); } .playbtn:hover, .playbtn:focus-visible { background: var(--wait); color: var(--ink); } }

	.filmdlg { padding: 0; border: 0; background: transparent; color: #fff; width: min(calc((100svh - 3rem) * 16 / 9), 1500px, 100vw - 2rem); max-width: none; max-height: none; overflow: visible; margin: auto; }
	.filmdlg::backdrop { background: rgb(4 8 56 / 0.86); }
	.in { position: relative; }
	.x { position: absolute; right: 0; top: -52px; z-index: 5; width: 48px; height: 48px; display: grid; place-items: center; background: transparent; border: 0; color: #fff; cursor: pointer; border-radius: 3px; }
	.x:hover { background: rgb(255 255 255 / 0.16); }
	.x:focus-visible { outline: 2px solid var(--wait); }
	.rotate { margin: 0.8rem 0 0; font-weight: 600; font-size: 1rem; text-align: center; color: #fff; }
	/* The dialog is a column on a phone: the close button sits above the video, and the whole thing is vertically centered by the browser. */
	@media (max-width: 640px) { .filmdlg { width: calc(100vw - 1rem); } .x { top: -52px; right: 0; } }
	@media (max-height: 520px) and (orientation: landscape) { .x { top: 6px; right: 6px; z-index: 5; background: rgb(8 12 70 / 0.7); } .filmdlg { width: min(calc((100svh - 1rem) * 16 / 9), 100vw - 1rem); } }
</style>
