<script lang="ts">
	// Custom film player. Idle cost: one poster image, no video bytes (preload="none" until first play).
	// First play creates nothing new: the <video> is in the DOM from the start with preload="none".
	import { onMount } from "svelte";
	import { CHAPTERS, FILM_DURATION, THUMB } from "../../data/film";

	let { poster, mp4, webm, captions, thumbs, label }: { poster: string; mp4: string; webm: string; captions: string; thumbs: string; label: string } = $props();

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
	let pipOk = $state(false);
	let coarse = $state(false);
	let panel = $state<"" | "chapters" | "speed">("");
	let hover = $state<{ x: number; t: number } | null>(null);
	let scrubbing = $state(false);
	let live = $state("");
	let hideTimer: ReturnType<typeof setTimeout> | undefined;
	let track: TextTrack | undefined;

	const RATES = [0.75, 1, 1.25, 1.5, 2];
	const fmt = (s: number) => `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
	const chapterAt = (t: number) => CHAPTERS.findLastIndex((c) => t >= c.t);
	const current = $derived(chapterAt(time));
	// Preview tile: the sprite is scaled down with the tile on narrow players (128x72 instead of 160x90).
	const thumbStyle = (t: number, k: number) => {
		const i = Math.min(Math.floor(t / THUMB.step), Math.ceil(FILM_DURATION / THUMB.step) - 1);
		const w = THUMB.w * k, h = THUMB.h * k;
		return `width:${w}px;height:${h}px;background-image:url(${thumbs});background-size:${THUMB.cols * w}px auto;background-position:-${(i % THUMB.cols) * w}px -${Math.floor(i / THUMB.cols) * h}px`;
	};

	function say(msg: string) {
		live = "";
		queueMicrotask(() => (live = msg));
	}

	async function play() {
		started = true;
		if (ended) video.currentTime = 0;
		try {
			await video.play();
		} catch {
			say("Playback was blocked. Press play again.");
		}
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
	async function fullscreen() {
		const v = video as HTMLVideoElement & { webkitEnterFullscreen?: () => void; webkitExitFullscreen?: () => void; webkitDisplayingFullscreen?: boolean };
		if (document.fullscreenElement) await document.exitFullscreen();
		// iPhone Safari: only the <video> itself can go fullscreen (native player), the custom bar cannot follow.
		else if (!root.requestFullscreen && v.webkitEnterFullscreen) v.webkitEnterFullscreen();
		else await root.requestFullscreen().catch(() => v.webkitEnterFullscreen?.());
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
		coarse = matchMedia("(pointer: coarse)").matches;
		pipOk = !!document.pictureInPictureEnabled && "requestPictureInPicture" in video;
		try {
			ccOn = localStorage.getItem("atlas-film-cc") === "1";
		} catch {}
		const onFs = () => {
			fs = document.fullscreenElement === root;
			if (!fs) screen.orientation?.unlock?.();
		};
		document.addEventListener("fullscreenchange", onFs);
		return () => {
			document.removeEventListener("fullscreenchange", onFs);
			clearTimeout(hideTimer);
		};
	});

	function onLoaded() {
		duration = video.duration || FILM_DURATION;
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
</script>

<div
	class="player"
	class:idle={!started}
	class:hide={!chromeVisible && playing && !panel}
	class:fs
	bind:this={root}
	role="group"
	aria-label={label}
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
		aria-label={label}
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

	{#if cue && ccOn && started}
		<p class="cue" class:lift={chromeVisible}><span>{cue}</span></p>
	{/if}

	{#if !started}
		<button type="button" class="big" onclick={play}>
			<span class="blk"><svg width="26" height="26" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 3.5v17l15-8.5z" /></svg><b>Play the film</b><small aria-label="1 minute 18 seconds">1:18</small></span>
		</button>
	{/if}

	{#if waiting && playing}<div class="spin" aria-hidden="true"></div>{/if}

	{#if ended}
		<div class="end">
			<p class="end-h display">That was the whole thing.</p>
			<p class="end-p">Install it in one line, or grab the .dmg.</p>
			<div class="end-b">
				<a class="btn solid" href="/#install" onclick={() => document.fullscreenElement && document.exitFullscreen()}>Install Atlas</a>
				<button type="button" class="btn" onclick={play}>Watch again</button>
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
				aria-valuetext={`${fmt(time)} of ${fmt(duration)}, chapter ${CHAPTERS[current]?.title ?? ""}`}
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
						<div class="th" style={thumbStyle(hover.t, (root?.clientWidth ?? 1000) < 640 ? 0.8 : 1)}></div>
						<span class="pt">{fmt(hover.t)}</span>
						<span class="pc">{CHAPTERS[chapterAt(hover.t)]?.title}</span>
					</div>
				{/if}
			</div>

			<div class="row">
				<button type="button" class="ic" onclick={toggle} aria-label={playing ? "Pause" : "Play"} aria-keyshortcuts="k Space">
					{#if playing}<svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 4h4v16H6zM14 4h4v16h-4z" /></svg>{:else}<svg width="22" height="22" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M6 3.5v17l15-8.5z" /></svg>{/if}
				</button>
				<button type="button" class="ic sk" onclick={() => skip(-5)} aria-label="Back 5 seconds" aria-keyshortcuts="j"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 12a8 8 0 1 0 3-6.2M4 4v4h4" /></svg><b>5</b></button>
				<button type="button" class="ic sk" onclick={() => skip(5)} aria-label="Forward 5 seconds" aria-keyshortcuts="l"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M20 12a8 8 0 1 1-3-6.2M20 4v4h-4" /></svg><b>5</b></button>
				<div class="vol">
					<button type="button" class="ic" onclick={() => setMuted(!muted)} aria-label={muted ? "Unmute" : "Mute"} aria-keyshortcuts="m">
						{#if muted || volume === 0}<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9h4l5-4v14l-5-4H4zM17 9l5 6M22 9l-5 6" /></svg>{:else}<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9h4l5-4v14l-5-4H4zM17 8a5 5 0 0 1 0 8" /></svg>{/if}
					</button>
					<input type="range" min="0" max="1" step="0.05" value={muted ? 0 : volume} oninput={(e) => setVolume(Number(e.currentTarget.value))} aria-label="Volume" />
				</div>
				<p class="tm" aria-hidden="true">{fmt(time)} <span>/ {fmt(duration)}</span></p>
				<p class="ch" aria-hidden="true">{CHAPTERS[current]?.title}</p>
				<span class="sp"></span>
				<div class="menu">
					<button type="button" class="ic txt" onclick={() => (panel = panel === "speed" ? "" : "speed")} aria-label={`Playback speed ${rate}x`} aria-expanded={panel === "speed"} aria-haspopup="true">{rate}x</button>
					{#if panel === "speed"}
						<ul class="pop" role="menu" aria-label="Speed">
							{#each RATES as r}<li role="none"><button type="button" role="menuitemradio" aria-checked={r === rate} onclick={() => setRate(r)}>{r}x</button></li>{/each}
						</ul>
					{/if}
				</div>
				<button type="button" class="ic txt" class:on={ccOn} onclick={() => setCaptions(!ccOn)} aria-label="Captions" aria-pressed={ccOn} aria-keyshortcuts="c">CC</button>
				<div class="menu">
					<button type="button" class="ic txt wide" onclick={() => (panel = panel === "chapters" ? "" : "chapters")} aria-expanded={panel === "chapters"} aria-haspopup="true">Chapters</button>
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
				{#if pipOk}<button type="button" class="ic" onclick={pip} aria-label="Picture in picture"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M3 5h18v14H3zM12 12h7v5h-7z" /></svg></button>{/if}
				<button type="button" class="ic" onclick={fullscreen} aria-label={fs ? "Exit full screen" : "Full screen"} aria-keyshortcuts="f"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" /></svg></button>
			</div>
		</div>
	{/if}

	<p class="sr-only" aria-live="polite">{live}</p>
</div>

<!-- Kept in the layout while playing (hidden, not removed) so the page below never jumps when the film starts -->
<ol class="chapters" class:gone={started} aria-hidden={started} aria-label="Film chapters" inert={started}>
		{#each CHAPTERS as c, i}
			<li><button type="button" onclick={() => (seek(c.t), play())}><time>{fmt(c.t)}</time>{c.title}</button></li>
		{/each}
	</ol>

<style>
	.player { position: relative; aspect-ratio: 16 / 9; width: 100%; background: #0b0c10; border-radius: 10px; overflow: hidden; box-shadow: 0 0 0 1px rgb(255 255 255 / 0.14), 0 40px 80px -30px rgb(3 6 60 / 0.8); color: #fff; user-select: none; -webkit-user-select: none; touch-action: manipulation; }
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
	.ic.txt.wide { padding: 0 0.8rem; }
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

	/* chapter list under the idle player: plain text, clickable */
	.chapters { list-style: none; margin: 1rem 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 0.4rem 1.6rem; }
	.chapters button { background: transparent; border: 0; color: var(--fg-2); font: 600 0.95rem var(--font); cursor: pointer; padding: 0.35rem 0; display: inline-flex; gap: 0.5rem; text-decoration: underline; text-decoration-color: transparent; text-underline-offset: 0.22em; text-decoration-thickness: 1.5px; }
	.chapters button:hover { color: #fff; text-decoration-color: currentColor; }
	.chapters.gone { visibility: hidden; }
	.chapters time { font: 500 0.82rem var(--mono); opacity: 0.7; padding-top: 0.12rem; }
	@media (pointer: coarse) { .chapters button { min-height: 44px; align-items: center; font-size: 1rem; } .chapters time { font-size: 0.875rem; padding-top: 0; } }

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
	@media (max-width: 640px) {
		.player { border-radius: 8px; }
		.ch, .vol { display: none; }
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
	@media (max-width: 640px) and (orientation: portrait) { .row { flex-wrap: nowrap; overflow-x: auto; scrollbar-width: none; } .row::-webkit-scrollbar { display: none; } }
	@media (prefers-reduced-motion: reduce) { .bar, .cue, .vol input { transition: none; } .spin { animation-duration: 2s; } }
</style>
