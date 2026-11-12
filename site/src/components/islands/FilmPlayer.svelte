<!-- Mounted by FilmDialog (a Svelte island), not by an .astro page: Astro only extracts CSS of components a page imports, so the player carries its own styles in its JS. -->
<svelte:options css="injected" />

<script lang="ts">
	// Custom film player, mounted inside the hero's dialog (FilmDialog). Idle cost: nothing but the poster (preload="none"
	// until the first play). The dialog's button starts it inside the visitor's tap, so sound is allowed.
	import { onMount } from "svelte";
	import { CHAPTERS, FILM_DURATION, chapterAt, filmClock, filmSeconds, fmt, needsRotate, thumbStyle } from "../../data/film";

	let { poster, mp4, webm, captions, thumbs, words, autostart }: { poster: string; mp4: string; webm: string; captions: string; thumbs: string; words: string; autostart?: { nativeFullscreen: boolean } } = $props();

	let root: HTMLDivElement;
	let video: HTMLVideoElement;
	let mounted = $state(false);
	let started = $state(false);
	let playing = $state(false);
	let ended = $state(false);
	let waiting = $state(false);
	let time = $state(0);
	let duration = $state(FILM_DURATION);
	let buffered = $state(0);
	let volume = $state(1);
	let muted = $state(false);
	let rate = $state(1);
	let ccOn = $state(false);
	let cue = $state("");
	let chromeVisible = $state(true);
	let fs = $state(false);
	let portrait = $state(false);
	let pipOk = $state(false);
	let coarse = $state(false);
	let panel = $state<"" | "chapters" | "speed" | "more">("");
	let hover = $state<{ x: number; t: number } | null>(null);
	let scrubbing = $state(false);
	let live = $state("");
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let track: TextTrack | undefined;

	const RATES = [0.75, 1, 1.25, 1.5, 2];
	const current = $derived(chapterAt(time));

	function say(msg: string) {
		live = "";
		queueMicrotask(() => (live = msg));
	}

	type NativeVideo = HTMLVideoElement & { webkitEnterFullscreen?: () => void; webkitExitFullscreen?: () => void };

	/** Start (or restart) playback. Must be called inside the user's gesture. `nativeFullscreen`: hand the video to the OS player first. */
	export async function play(opts: { nativeFullscreen?: boolean } = {}) {
		started = true;
		if (ended) video.currentTime = 0;
		try {
			await video.play();
			if (opts.nativeFullscreen) await fullscreen(true);
		} catch {
			say("Playback was blocked. Press play again.");
		}
	}
	/** Pause and rewind to the poster (the dialog closed). */
	export function stop() {
		video?.pause();
		if (video && video.currentTime > 0) video.currentTime = 0;
		time = 0;
		ended = false;
		started = false;
		panel = "";
	}
	const toggle = () => (video.paused || video.ended ? play() : video.pause());
	function seek(t: number) {
		const to = Math.max(0, Math.min(duration - 0.05, t));
		video.currentTime = to;
		time = to;
		ended = false;
	}
	const skip = (d: number) => {
		seek(video.currentTime + d);
		say(`${d > 0 ? "Forward" : "Back"} 5 seconds, ${fmt(video.currentTime)}`);
	};
	function setMuted(m: boolean) {
		video.muted = m;
		muted = m;
		say(m ? "Muted" : "Sound on");
	}
	function setVolume(v: number) {
		video.volume = v;
		volume = v;
		if (v > 0 && video.muted) setMuted(false);
	}
	function setCaptions(on: boolean) {
		ccOn = on;
		try {
			localStorage.setItem("atlas-film-cc", on ? "1" : "0");
		} catch {}
		if (track) track.mode = on ? "hidden" : "disabled";
		if (!on) cue = "";
		say(on ? "Captions on" : "Captions off");
	}
	function setRate(r: number) {
		video.playbackRate = r;
		rate = r;
		panel = "";
		say(`Speed ${r}x`);
	}
	/** `native`: phone tap, prefer the <video>'s own fullscreen (iOS rotates it and draws the OS controls); otherwise the player goes fullscreen. */
	async function fullscreen(native = false) {
		const v = video as NativeVideo;
		if (document.fullscreenElement) return document.exitFullscreen();
		// iPhone Safari: only the <video> itself can go fullscreen (native player), the custom bar cannot follow.
		if ((native || !root.requestFullscreen) && v.webkitEnterFullscreen) return v.webkitEnterFullscreen();
		await root.requestFullscreen().catch(() => v.webkitEnterFullscreen?.());
		// landscape on phones is what fullscreen is for
		try {
			await (screen.orientation as ScreenOrientation & { lock?: (o: string) => Promise<void> }).lock?.("landscape");
		} catch {}
	}
	async function pip() {
		try {
			if (document.pictureInPictureElement) await document.exitPictureInPicture();
			else await video.requestPictureInPicture();
		} catch {}
	}

	// Mouse: click toggles play. Touch: first tap reveals the controls, tapping a visible surface toggles play.
	function onSurface() {
		if (!started) return play();
		if (coarse && playing && !chromeVisible) return poke();
		toggle();
	}

	function poke() {
		chromeVisible = true;
		clearTimeout(hideTimer);
		if (playing && !scrubbing && !panel) hideTimer = setTimeout(() => (chromeVisible = false), 2600);
	}
	$effect(() => {
		playing;
		panel;
		scrubbing;
		poke();
	});

	function fromPointer(e: PointerEvent, el: HTMLElement) {
		const r = el.getBoundingClientRect();
		const x = Math.max(0, Math.min(r.width, e.clientX - r.left));
		return { x, t: (x / r.width) * duration };
	}
	function onBarMove(e: PointerEvent) {
		const h = fromPointer(e, e.currentTarget as HTMLElement);
		hover = h;
		if (scrubbing) seek(h.t);
	}
	function onBarDown(e: PointerEvent) {
		const el = e.currentTarget as HTMLElement;
		el.setPointerCapture(e.pointerId);
		hover = fromPointer(e, el);
		scrubbing = true;
		started = true;
		seek(fromPointer(e, el).t);
	}
	function onBarUp() {
		scrubbing = false;
		if (coarse) hover = null;
	}
	function onBarKey(e: KeyboardEvent) {
		const step = e.shiftKey ? 10 : 5;
		if (e.key === "ArrowLeft" || e.key === "ArrowDown") (e.preventDefault(), e.stopPropagation(), skip(-step));
		else if (e.key === "ArrowRight" || e.key === "ArrowUp") (e.preventDefault(), e.stopPropagation(), skip(step));
		else if (e.key === "Home") (e.preventDefault(), seek(0));
		else if (e.key === "End") (e.preventDefault(), seek(duration));
	}

	function onKey(e: KeyboardEvent) {
		if (e.metaKey || e.ctrlKey || e.altKey) return;
		const tag = (e.target as HTMLElement).tagName;
		if (tag === "INPUT" && (e.target as HTMLInputElement).type === "text") return;
		const k = e.key.toLowerCase();
		const onButton = tag === "BUTTON";
		let handled = true;
		if (k === " " && !onButton) toggle();
		else if (k === "k") toggle();
		else if (k === "j") skip(-5);
		else if (k === "l") skip(5);
		else if (k === "arrowleft") skip(-5);
		else if (k === "arrowright") skip(5);
		else if (k === "m") setMuted(!video.muted);
		else if (k === "c") setCaptions(!ccOn);
		else if (k === "f") fullscreen();
		else if (k === "escape" && panel) panel = "";
		else if (/^[0-9]$/.test(k)) {
			seek((Number(k) / 10) * duration);
			say(`${Number(k) * 10} percent`);
		} else handled = false;
		if (handled) {
			e.preventDefault();
			poke();
		}
	}

	onMount(() => {
		mounted = true;
		// Mounted by the tap that opened the dialog (same task, still inside the user gesture): start the film now.
		if (autostart) void play(autostart);
		coarse = matchMedia("(pointer: coarse)").matches;
		pipOk = !!document.pictureInPictureEnabled && "requestPictureInPicture" in video;
		try {
			ccOn = localStorage.getItem("atlas-film-cc") === "1";
		} catch {}
		const onFs = () => {
			fs = document.fullscreenElement === root;
			if (!fs) screen.orientation?.unlock?.();
		};
		const mq = matchMedia("(orientation: portrait)");
		const onOri = () => (portrait = mq.matches);
		onOri();
		mq.addEventListener("change", onOri);
		document.addEventListener("fullscreenchange", onFs);
		return () => {
			document.removeEventListener("fullscreenchange", onFs);
			mq.removeEventListener("change", onOri);
			clearTimeout(hideTimer);
		};
	});

	function onLoaded() {
		duration = video.duration || FILM_DURATION;
		mediaLoaded = !!video.duration;
		track = video.textTracks[0];
		if (track) {
			track.mode = ccOn ? "hidden" : "disabled";
			track.oncuechange = () => {
				const c = track!.activeCues?.[0] as VTTCue | undefined;
				cue = c?.text ?? "";
			};
		}
	}
	function onProgress() {
		const b = video.buffered;
		for (let i = 0; i < b.length; i++) if (b.start(i) <= video.currentTime && b.end(i) >= video.currentTime) buffered = b.end(i);
	}
	/** The film's length as everything else on the page states it: the published length until the media says otherwise, one rule (filmClock). */
	const total = $derived(filmClock(mediaLoaded ? duration : filmSeconds(FILM_DURATION)));
	let mediaLoaded = $state(false);
</script>

<div
	class="player"
	class:idle={!started}
	class:hide={!chromeVisible && playing && !panel}
	class:fs
	bind:this={root}
	role="group"
	aria-label={`Atlas product film, ${words}`}
	onpointermove={poke}
	onfocusin={poke}
	onkeydown={onKey}
>
	<video
		bind:this={video}
		poster={mounted ? poster : undefined}
		preload="none"
		playsinline
		crossorigin="anonymous"
		aria-label={`Atlas product film, ${words}`}
		onclick={onSurface}
		ondblclick={() => !coarse && fullscreen()}
		onplay={() => ((playing = true), (ended = false), say("Playing"))}
		onpause={() => ((playing = false), !video.ended && say("Paused"))}
		onended={() => ((playing = false), (ended = true), (chromeVisible = true))}
		onwaiting={() => (waiting = true)}
		onplaying={() => (waiting = false)}
		ontimeupdate={() => !scrubbing && (time = video.currentTime)}
		onprogress={onProgress}
		onloadedmetadata={onLoaded}
		onvolumechange={() => ((volume = video.volume), (muted = video.muted))}
	>
		<source src={webm} type="video/webm" />
		<source src={mp4} type="video/mp4" />
		<track kind="captions" src={captions} srclang="en" label="English" />
	</video>

	{#if needsRotate({ fullscreen: fs, portrait })}
		<p class="turn" role="status"><svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><rect x="7" y="2" width="10" height="20" rx="2" /><path d="M3 9v6M21 9v6" /></svg>Turn your phone sideways to fill the screen</p>
	{/if}

	{#if cue && ccOn && started}
		<p class="cue" class:lift={chromeVisible}><span>{cue}</span></p>
	{/if}

	{#if !started}
		<button type="button" class="big" onclick={() => play()} aria-label={`Play the film, ${words}`}>
			<span class="blk"><svg width="26" height="26" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 3.5v17l15-8.5z" /></svg><b>Play</b><small>{total}</small></span>
		</button>
	{/if}

	{#if waiting && playing}<div class="spin" aria-hidden="true"></div>{/if}

	{#if ended}
		<div class="end">
			<p class="end-h display">That was the whole thing.</p>
			<p class="end-p">Install it in one line, or download the installer for your platform.</p>
			<div class="end-b">
				<a class="btn solid" href="/#install" onclick={() => document.fullscreenElement && document.exitFullscreen()}>Install Atlas</a>
				<button type="button" class="btn" onclick={() => play()}>Watch again</button>
			</div>
		</div>
	{/if}

	{#if started && !ended}
		<div class="bar" role="toolbar" aria-label="Video controls">
			<div
				class="track"
				role="slider"
				tabindex="0"
				aria-label="Seek"
				aria-valuemin="0"
				aria-valuemax={Math.round(duration)}
				aria-valuenow={Math.round(time)}
				aria-valuetext={`${fmt(time)} of ${total}, chapter ${CHAPTERS[current]?.title ?? ""}`}
				onpointermove={onBarMove}
				onpointerdown={onBarDown}
				onpointerup={onBarUp}
				onpointercancel={onBarUp}
				onpointerleave={() => !scrubbing && (hover = null)}
				onkeydown={onBarKey}
			>
				<div class="rail"></div>
				<div class="buf" style={`width:${(buffered / duration) * 100}%`}></div>
				<div class="fill" style={`width:${(time / duration) * 100}%`}></div>
				{#each CHAPTERS.slice(1) as c}<i class="tick" style={`left:${(c.t / duration) * 100}%`}></i>{/each}
				<div class="knob" style={`left:${(time / duration) * 100}%`}></div>
				{#if hover}
					<div class="pv" style={`left:${Math.max(88, Math.min(hover.x, (root?.clientWidth ?? 0) - 88))}px`} aria-hidden="true">
						<div class="th" style={thumbStyle(thumbs, hover.t, (root?.clientWidth ?? 1000) < 640 ? 0.8 : 1)}></div>
						<span class="pt">{fmt(hover.t)}</span>
						<span class="pc">{CHAPTERS[chapterAt(hover.t)]?.title}</span>
					</div>
				{/if}
			</div>

			<div class="row">
				<button type="button" class="ic" onclick={toggle} aria-label={playing ? "Pause" : "Play"} aria-keyshortcuts="k Space">
					{#if playing}<svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 4h4v16H6zM14 4h4v16h-4z" /></svg>{:else}<svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 3.5v17l15-8.5z" /></svg>{/if}
				</button>
				<button type="button" class="ic sk wide-only" onclick={() => skip(-5)} aria-label="Back 5 seconds" aria-keyshortcuts="j"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 12a8 8 0 1 0 3-6.2M4 4v4h4" /></svg><b>5</b></button>
				<button type="button" class="ic sk wide-only" onclick={() => skip(5)} aria-label="Forward 5 seconds" aria-keyshortcuts="l"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M20 12a8 8 0 1 1-3-6.2M20 4v4h-4" /></svg><b>5</b></button>
				<div class="vol wide-only">
					<button type="button" class="ic" onclick={() => setMuted(!muted)} aria-label={muted ? "Unmute" : "Mute"} aria-keyshortcuts="m">
						{#if muted || volume === 0}<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9h4l5-4v14l-5-4H4zM17 9l5 6M22 9l-5 6" /></svg>{:else}<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9h4l5-4v14l-5-4H4zM17 8a5 5 0 0 1 0 8" /></svg>{/if}
					</button>
					<input type="range" min="0" max="1" step="0.05" value={muted ? 0 : volume} oninput={(e) => setVolume(Number(e.currentTarget.value))} aria-label="Volume" />
				</div>
				<p class="tm" aria-hidden="true">{fmt(time)} <span>/ {total}</span></p>
				<p class="ch wide-only" aria-hidden="true">{CHAPTERS[current]?.title}</p>
				<span class="sp"></span>
				<div class="menu wide-only">
					<button type="button" class="ic txt" onclick={() => (panel = panel === "speed" ? "" : "speed")} aria-label={`Playback speed ${rate}x`} aria-expanded={panel === "speed"} aria-haspopup="true">{rate}x</button>
					{#if panel === "speed"}
						<ul class="pop" role="menu" aria-label="Speed">
							{#each RATES as r}<li role="none"><button type="button" role="menuitemradio" aria-checked={r === rate} onclick={() => setRate(r)}>{r}x</button></li>{/each}
						</ul>
					{/if}
				</div>
				<button type="button" class="ic txt cc-row" class:on={ccOn} onclick={() => setCaptions(!ccOn)} aria-label="Captions" aria-pressed={ccOn} aria-keyshortcuts="c">CC</button>
				<div class="menu">
					<button type="button" class="ic txt chbtn" onclick={() => (panel = panel === "chapters" ? "" : "chapters")} aria-label="Chapters" aria-expanded={panel === "chapters"} aria-haspopup="true"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h10" /></svg><span class="wide-only">Chapters</span></button>
					{#if panel === "chapters"}
						{#if coarse}<button type="button" class="scrim" aria-label="Close chapters" onclick={() => (panel = "")}></button>{/if}
						<ul class="pop chap" class:sheet={coarse} role="menu" aria-label="Chapters">
							{#if coarse}<li class="sh" role="presentation"><b>Chapters</b><button type="button" class="x" aria-label="Close chapters" onclick={() => (panel = "")}>×</button></li>{/if}
							{#each CHAPTERS as c, i}
								<li role="none"><button type="button" role="menuitem" class:cur={i === current} onclick={() => (seek(c.t), (panel = ""), play())}><span>{c.title}</span><time>{fmt(c.t)}</time></button></li>
							{/each}
						</ul>
					{/if}
				</div>
				<div class="menu narrow-only">
					<button type="button" class="ic" onclick={() => (panel = panel === "more" ? "" : "more")} aria-label="More controls" aria-expanded={panel === "more"} aria-haspopup="true"><svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="1.9" /><circle cx="12" cy="12" r="1.9" /><circle cx="19" cy="12" r="1.9" /></svg></button>
					{#if panel === "more"}
						<button type="button" class="scrim" aria-label="Close menu" onclick={() => (panel = "")}></button>
						<ul class="pop sheet" role="menu" aria-label="More controls">
							<li class="sh" role="presentation"><b>Controls</b><button type="button" class="x" aria-label="Close menu" onclick={() => (panel = "")}>×</button></li>
							<li role="none"><button type="button" role="menuitem" onclick={() => (skip(-5), (panel = ""))}><span>Back 5 seconds</span></button></li>
							<li role="none"><button type="button" role="menuitem" onclick={() => (skip(5), (panel = ""))}><span>Forward 5 seconds</span></button></li>
							<li role="none"><button type="button" role="menuitemcheckbox" aria-checked={muted} onclick={() => (setMuted(!muted), (panel = ""))}><span>{muted ? "Unmute" : "Mute"}</span></button></li>
							<li role="none" class="cc-more"><button type="button" role="menuitemcheckbox" aria-checked={ccOn} onclick={() => (setCaptions(!ccOn), (panel = ""))}><span>Captions</span></button></li>
							{#each RATES as r}<li role="none"><button type="button" role="menuitemradio" aria-checked={r === rate} onclick={() => setRate(r)}><span>Speed {r}x</span></button></li>{/each}
							{#if pipOk}<li role="none"><button type="button" role="menuitem" onclick={() => (pip(), (panel = ""))}><span>Picture in picture</span></button></li>{/if}
						</ul>
					{/if}
				</div>
				{#if pipOk}<button type="button" class="ic wide-only" onclick={pip} aria-label="Picture in picture"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 5h18v14H3zM12 12h7v5h-7z" /></svg></button>{/if}
				<button type="button" class="ic" onclick={() => fullscreen()} aria-label={fs ? "Exit full screen" : "Full screen"} aria-keyshortcuts="f"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" /></svg></button>
			</div>
		</div>
	{/if}

	<p class="sr-only" aria-live="polite">{live}</p>
</div>


<style>
	.player { position: relative; container: player / inline-size; aspect-ratio: 16 / 9; width: 100%; background: #0b0c10; border-radius: 10px; overflow: hidden; box-shadow: 0 0 0 1px rgb(255 255 255 / 0.14), 0 40px 80px -30px rgb(3 6 60 / 0.8); color: #fff; user-select: none; -webkit-user-select: none; touch-action: manipulation; }
	.player.fs { border-radius: 0; box-shadow: none; }
	.player.hide { cursor: none; }
	video { position: absolute; inset: 0; width: 100%; height: 100%; object-fit: contain; background: #0b0c10; }
	.idle video { object-fit: cover; }

	/* idle: the play affordance is the cursor block from the buttons, made huge */
	.big { position: absolute; inset: 0; display: flex; align-items: flex-end; padding: clamp(1rem, 3vw, 2.6rem); border: 0; cursor: pointer; color: var(--ink); background: linear-gradient(0deg, rgb(11 22 114 / 0.9), rgb(11 22 114 / 0) 50%); }
	.blk { display: inline-flex; align-items: center; gap: 1rem; background: #fff; padding: 1rem 1.5rem 1rem 1.1rem; border-radius: 3px; font: 800 clamp(1.3rem, 3vw, 2.4rem)/1 var(--font); font-stretch: 70%; transition: background 0.12s, transform 0.12s; }
	.blk svg { width: clamp(22px, 3vw, 34px); height: auto; color: var(--blue); }
	.blk small { font: 600 0.9rem/1 var(--mono); color: var(--ink-2); }
	.big:hover .blk, .big:focus-visible .blk { background: var(--wait); }
	.big:active .blk { transform: translateY(2px); }
	.big:focus-visible { outline: 3px solid var(--wait); outline-offset: -6px; }

	.turn { position: absolute; left: 0; right: 0; top: 18%; margin: 0; display: grid; justify-items: center; gap: 0.5rem; padding-inline: 1.5rem; text-align: center; font: 700 1.1rem/1.3 var(--font); pointer-events: none; text-shadow: 0 1px 8px rgb(0 0 0 / 0.8); }
	.spin { position: absolute; left: 50%; top: 50%; width: 44px; height: 44px; margin: -22px; border: 4px solid rgb(255 255 255 / 0.3); border-top-color: #fff; border-radius: 50%; animation: sp 0.8s linear infinite; pointer-events: none; }
	@keyframes sp { to { transform: rotate(360deg); } }

	/* captions: our own rendering, mono like the product's terminal */
	.cue { position: absolute; left: 0; right: 0; bottom: 1.6rem; margin: 0; display: flex; justify-content: center; padding-inline: 1rem; pointer-events: none; transition: transform 0.2s; }
	.cue.lift { transform: translateY(-4.6rem); }
	.cue span { background: #0b0c10; color: #fff; font: 500 clamp(0.85rem, 1.7vw, 1.35rem)/1.45 var(--mono); padding: 0.25em 0.6em; max-width: 46ch; text-align: center; box-decoration-break: clone; }

	/* controls */
	.bar { position: absolute; left: 0; right: 0; bottom: 0; padding: 2.4rem clamp(0.6rem, 1.6vw, 1.2rem) clamp(0.5rem, 1.2vw, 0.9rem); background: linear-gradient(0deg, rgb(8 12 70 / 0.94), rgb(8 12 70 / 0)); transition: opacity 0.22s, transform 0.22s; }
	.hide .bar { opacity: 0; transform: translateY(8px); pointer-events: none; }
	.track { position: relative; height: 22px; cursor: pointer; touch-action: none; display: flex; align-items: center; }
	.track:focus-visible { outline: 2px solid var(--wait); outline-offset: 2px; }
	.rail, .buf, .fill { position: absolute; left: 0; height: 6px; top: 8px; transition: height 0.12s, top 0.12s; }
	.rail { right: 0; background: rgb(255 255 255 / 0.28); }
	.buf { background: rgb(255 255 255 / 0.5); }
	.fill { background: var(--wait); }
	.track:hover .rail, .track:hover .buf, .track:hover .fill, .track:focus-visible .rail, .track:focus-visible .buf, .track:focus-visible .fill { height: 10px; top: 6px; }
	.tick { position: absolute; top: 8px; width: 3px; height: 6px; margin-left: -1.5px; background: #0b1672; pointer-events: none; }
	.track:hover .tick { height: 10px; top: 6px; }
	.knob { position: absolute; top: 4px; width: 4px; height: 14px; margin-left: -2px; background: #fff; pointer-events: none; }
	.pv { position: absolute; bottom: 30px; transform: translateX(-50%); display: grid; justify-items: center; gap: 0.3rem; pointer-events: none; }
	.th { background-repeat: no-repeat; background-color: #0b0c10; box-shadow: 0 0 0 2px #fff; }
	.pt { font: 700 0.85rem/1 var(--mono); background: #fff; color: var(--ink); padding: 0.25rem 0.4rem; }
	.pc { font: 700 0.85rem/1 var(--font); font-stretch: 85%; background: var(--wait); color: var(--ink); padding: 0.25rem 0.4rem; }

	.row { display: flex; align-items: center; gap: 0.15rem; margin-top: 0.2rem; }
	.ic { position: relative; display: inline-grid; place-items: center; min-width: 44px; height: 44px; padding: 0 0.5rem; background: transparent; border: 0; color: #fff; cursor: pointer; border-radius: 3px; }
	.ic:hover { background: rgb(255 255 255 / 0.16); }
	.ic:focus-visible { outline: 2px solid var(--wait); outline-offset: -2px; }
	.ic.txt { font: 800 0.95rem/1 var(--font); font-stretch: 85%; }
	.ic.txt.chbtn { gap: 0.4rem; padding: 0 0.8rem; }
	@container player (max-width: 560px) { .ic.txt.chbtn { padding: 0 0.2rem; } }
	.ic.on { background: #fff; color: var(--ink); }
	.sk b { position: absolute; font: 800 0.58rem/1 var(--font); top: 50%; left: 50%; transform: translate(-50%, -38%); }
	.vol { display: flex; align-items: center; }
	.vol input { width: 0; opacity: 0; transition: width 0.2s, opacity 0.2s; accent-color: var(--wait); height: 44px; margin: 0; }
	.vol:hover input, .vol:focus-within input { width: 80px; opacity: 1; margin-right: 0.4rem; }
	.tm { margin: 0 0.5rem; font: 700 0.9rem/1 var(--mono); white-space: nowrap; } .tm span { opacity: 0.65; font-weight: 500; }
	.ch { margin: 0 0.4rem; font: 700 0.95rem/1 var(--font); font-stretch: 85%; opacity: 0.9; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
	.sp { flex: 1; }
	.menu { position: relative; }
	.pop { position: absolute; right: 0; bottom: 52px; margin: 0; padding: 0.3rem; list-style: none; background: #fff; color: var(--ink); min-width: 9rem; border-radius: 3px; box-shadow: 4px 4px 0 var(--blue-deep); }
	.pop button { display: flex; justify-content: space-between; gap: 1.5rem; width: 100%; min-height: 40px; align-items: center; padding: 0 0.7rem; background: transparent; border: 0; color: inherit; font: 700 0.95rem/1 var(--font); font-stretch: 90%; cursor: pointer; text-align: left; white-space: nowrap; }
	.pop button:hover, .pop button:focus-visible { background: var(--ink); color: #fff; outline: none; }
	.pop button[aria-checked="true"], .pop button.cur { box-shadow: inset 4px 0 0 var(--blue); }
	.pop time { font: 500 0.82rem var(--mono); opacity: 0.7; }
	.chap { min-width: 16rem; }

	/* end screen */
	.end { position: absolute; inset: 0; display: grid; align-content: center; justify-items: start; gap: 1rem; padding: clamp(1.2rem, 5vw, 4rem); background: var(--blue); }
	.end-h { font-size: clamp(2.2rem, 7vw, 6rem); max-width: 14ch; }
	.end-p { margin: 0; max-width: 38ch; color: var(--fg-2); font-size: clamp(0.95rem, 1.6vw, 1.2rem); }
	.end-b { display: flex; gap: 0.8rem; flex-wrap: wrap; margin-top: 0.4rem; }


	/* Touch + phones. Controls: 44-48px targets, a 44px-tall scrub strip, chapters as a bottom sheet, no hover-only UI. */
	@media (pointer: coarse) {
		.track { height: 44px; }
		.rail, .buf, .fill { top: 19px; height: 6px; }
		.tick { top: 19px; }
		.knob { top: 13px; width: 6px; height: 18px; margin-left: -3px; border-radius: 3px; }
		.vol input { display: none; }
		.ic { min-width: 48px; height: 48px; }
		.pv { bottom: 52px; }
		.blk { padding: 0.9rem 1.2rem 0.9rem 0.9rem; }
		/* the speed menu is a small popover; chapters become a sheet over the whole player */
		.pop { bottom: 54px; }
		.pop button { min-height: 48px; font-size: 1rem; }
		.sheet { position: fixed; left: 0; right: 0; bottom: 0; top: auto; z-index: 90; min-width: 0; max-height: 72vh; max-height: 72dvh; overflow-y: auto; overscroll-behavior: contain; margin: 0; padding: 0 0 calc(0.6rem + env(safe-area-inset-bottom)); border-radius: 14px 14px 0 0; box-shadow: 0 -12px 40px rgb(3 6 60 / 0.6); }
		.sheet .sh { display: flex; align-items: center; justify-content: space-between; position: sticky; top: 0; background: #fff; padding: 0.5rem 0.4rem 0.5rem 1rem; border-bottom: 1px solid rgb(12 16 56 / 0.16); font: 800 1.2rem/1 var(--font); font-stretch: 75%; }
		.sheet .x { width: 48px; min-height: 48px; justify-content: center; font-size: 1.8rem; }
		.sheet button { min-height: 52px; padding-inline: 1rem; }
		.scrim { position: fixed; inset: 0; z-index: 89; width: 100%; height: 100%; padding: 0; border: 0; background: rgb(8 12 70 / 0.6); cursor: default; }
	}
	/* Narrow players (a phone's column, a small window): the row holds only play, time, CC, chapters, more, full screen
	   (six 44px targets plus the clock fit in 320px); everything else lives in the "more" sheet. Driven by the PLAYER's
	   width, not the viewport's, so it holds wherever the dialog ends up. */
	@container player (max-width: 560px) {
		.wide-only { display: none !important; }
		.bar { padding: 2rem 0.5rem calc(0.3rem + env(safe-area-inset-bottom)); }
		.fs .bar { padding-inline: max(0.5rem, env(safe-area-inset-left)) max(0.5rem, env(safe-area-inset-right)); }
		.tm { margin: 0 0.2rem; font-size: 0.8125rem; }
		.ic { min-width: 44px; padding: 0 0.2rem; }
		.ic.txt { font-size: 1rem; }
		.cue span { font-size: 0.9375rem; }
		.cue.lift { transform: translateY(-5.6rem); }
		.big { padding: 0.7rem; }
		.blk { font-size: 1.35rem; gap: 0.7rem; }
		.end { padding: 1rem; gap: 0.6rem; align-content: center; }
		.end-h { font-size: clamp(1.7rem, 9vw, 2.6rem); }
		.end-p { font-size: 0.9375rem; }
		.end-b .btn { padding: 0.6rem 0.9rem; min-height: 44px; }
	}
	/* 320px phones: captions move into the "more" sheet so play, clock, chapters, more and full screen fit at 44px each */
	@container player (max-width: 360px) { .cc-row { display: none !important; } .tm { margin: 0 0.1rem; } .row { gap: 0; } .bar { padding-inline: 0.25rem; } }
	@container player (min-width: 361px) { .cc-more { display: none !important; } }
	@container player (min-width: 561px) { .narrow-only { display: none !important; } }
	@media (max-width: 640px) { .player { border-radius: 8px; } }
	@media (prefers-reduced-motion: reduce) { .bar, .cue, .vol input { transition: none; } .spin { animation-duration: 2s; } }
</style>
