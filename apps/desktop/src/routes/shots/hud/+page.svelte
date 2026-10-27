<script lang="ts">
	// Recording HUD (label `shots-hud`; a 120x120 pill, widened for the trim step). Rust excludes this window from
	// the capture, so it may stay visible while recording. Keys: Esc = discard (countdown/recording/paused),
	// Enter = stop, P = pause/resume; in the trim step Enter saves (Esc does nothing: Discard is a button); after the
	// end Esc closes and Enter opens the file. Saving ignores every key except Esc while a GIF is being encoded.
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { Pause, Play, Square, X } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		formatSecs,
		fractionOf,
		keyAction,
		moveHandle,
		saveArgs,
		secsAt,
		view,
		type HudState,
		type KeyAction,
		MIN_TRIM_SECS,
	} from "#lib/shots/record.ts";

	let st = $state<HudState>({ phase: "idle", elapsedMs: 0, maxMs: 0, kind: "recording" });
	let failure = $state<string | null>(null);
	let v = $derived(view(st));

	// The trim step: the two handles (seconds) over the poster frames; reset whenever a new clip arrives.
	let range = $state<[number, number]>([0, 0]);
	let track = $state<HTMLDivElement | null>(null);
	let dragging = $state<"start" | "end" | null>(null);
	let secs = $derived(st.trim?.secs ?? 0);
	$effect(() => {
		range = [0, st.trim?.secs ?? 0];
	});

	/** One command per action; a rejection is shown, never swallowed. */
	async function act(a: KeyAction) {
		failure = null;
		try {
			if (a === "pause") await invoke("shots_record_pause");
			else if (a === "resume") await invoke("shots_record_resume");
			else if (a === "stop") await invoke("shots_record_stop");
			else if (a === "cancel") await invoke("shots_record_cancel");
			else if (a === "save") await invoke("shots_record_save", saveArgs(secs, range));
			else if (a === "open" && st.entry) {
				await invoke("shots_history_open", { path: st.entry.path });
				await getCurrentWindow().hide();
			} else await getCurrentWindow().hide();
		} catch (err) {
			failure = errText(err);
		}
	}

	async function discard() {
		failure = null;
		try {
			await invoke("shots_record_discard");
		} catch (err) {
			failure = errText(err);
		}
	}

	function onKey(e: KeyboardEvent) {
		// a focused handle owns the arrow keys; Enter on a button is the button's
		if ((e.target as HTMLElement | null)?.closest?.("button, [role=slider]") && e.key === "Enter") return;
		const a = keyAction(
			e.key,
			st.phase,
			{ meta: e.metaKey, ctrl: e.ctrlKey, alt: e.altKey },
			{ encoding: v.encode !== null },
		);
		if (!a) return;
		e.preventDefault();
		void act(a);
	}

	function drag(which: "start" | "end", e: PointerEvent) {
		dragging = which;
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
	}
	function dragMove(e: PointerEvent) {
		if (!dragging || !track) return;
		const r = track.getBoundingClientRect();
		range = moveHandle(dragging, secsAt(e.clientX, r.left, r.width, secs), range, secs);
	}
	function dragEnd() {
		dragging = null;
	}
	/** Arrow keys move a focused handle by a tenth of a second (a second with Shift). */
	function handleKey(which: "start" | "end", e: KeyboardEvent) {
		const step = e.shiftKey ? 1 : 0.1;
		const dir = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
		if (!dir) return;
		e.preventDefault();
		e.stopPropagation();
		range = moveHandle(which, range[which === "start" ? 0 : 1] + dir * step, range, secs);
	}

	onMount(() => {
		let off: (() => void) | undefined;
		let gone = false;
		void listen<HudState>("shots://record", (e) => {
			st = e.payload;
			failure = null;
		}).then((f) => (gone ? f() : (off = f)));
		void invoke<HudState>("shots_record_state")
			.then((s) => (st = s))
			.catch((err) => (failure = errText(err)));
		window.addEventListener("keydown", onKey);
		return () => {
			gone = true;
			off?.();
			window.removeEventListener("keydown", onKey);
		};
	});
</script>

<svelte:head><title>Atlas Recording</title></svelte:head>

<div class="hud" data-phase={st.phase} data-tone={v.tone} data-tauri-drag-region>
	{#if st.phase === "countdown" && v.digit !== null}
		<div class="count" aria-live="polite" aria-label="Recording starts in {v.digit}">
			<span class="digit" aria-hidden="true">{v.digit}</span>
			<button class="x" aria-label="Cancel recording" onclick={() => act("cancel")}>
				<X size={14} aria-hidden="true" />
			</button>
		</div>
	{:else if v.showTrim && st.trim}
		<div class="trim" role="group" aria-label="Trim the recording">
			<div
				class="track"
				bind:this={track}
				onpointermove={dragMove}
				onpointerup={dragEnd}
				onpointercancel={dragEnd}
			>
				<div class="posters" aria-hidden="true">
					{#each st.trim.posters as p, i (i)}<img src={p} alt="" draggable="false" />{/each}
				</div>
				<div
					class="shade"
					aria-hidden="true"
					style="left: 0; width: {fractionOf(range[0], secs) * 100}%"
				></div>
				<div
					class="shade"
					aria-hidden="true"
					style="right: 0; width: {(1 - fractionOf(range[1], secs)) * 100}%"
				></div>
				<div
					class="handle"
					role="slider"
					tabindex="0"
					aria-label="Trim start"
					aria-valuemin={0}
					aria-valuemax={Math.max(0, range[1] - MIN_TRIM_SECS)}
					aria-valuenow={range[0]}
					aria-valuetext={formatSecs(range[0])}
					style="left: {fractionOf(range[0], secs) * 100}%"
					onpointerdown={(e) => drag("start", e)}
					onkeydown={(e) => handleKey("start", e)}
				></div>
				<div
					class="handle"
					role="slider"
					tabindex="0"
					aria-label="Trim end"
					aria-valuemin={range[0] + MIN_TRIM_SECS}
					aria-valuemax={secs}
					aria-valuenow={range[1]}
					aria-valuetext={formatSecs(range[1])}
					style="left: {fractionOf(range[1], secs) * 100}%"
					onpointerdown={(e) => drag("end", e)}
					onkeydown={(e) => handleKey("end", e)}
				></div>
			</div>
			<p class="times" aria-live="polite">
				{formatSecs(range[0])} – {formatSecs(range[1])} · keeps {formatSecs(range[1] - range[0])}
			</p>
			{#if failure}<p class="note" role="alert">{failure}</p>{/if}
			{#each v.notes as note (note)}<p class="note" role="status">{note}</p>{/each}
			<div class="btns">
				<button class="b text" onclick={discard}>Discard</button>
				<button class="b text primary" onclick={() => act("save")}>Save</button>
			</div>
		</div>
	{:else if st.phase !== "idle"}
		<div class="pill" role="group" aria-label="Recording controls" aria-busy={v.progress}>
			<div class="top">
				<span class="dot" data-pulse={v.pulse} aria-hidden="true"></span>
				<span class="timer" aria-label={v.label}>{v.timer}</span>
			</div>
			<p class="label" aria-live="polite" title={failure ?? v.label}>{failure ?? v.label}</p>
			{#if v.estimate}<p class="est" aria-label="Estimated file size">{v.estimate}</p>{/if}
			{#if v.remaining}<p class="left" role="status">{v.remaining}</p>{/if}
			{#if v.progress}<div class="bar" role="progressbar" aria-label="Saving"></div>{/if}
			{#if v.encode !== null}
				<progress class="enc" aria-label="Encoding GIF" max="1" value={v.encode}></progress>
				<p class="hint">Esc to cancel</p>
			{/if}
			{#each v.notes as note (note)}<p class="note" role="status">{note}</p>{/each}
			<div class="btns">
				{#if v.showPause}
					<button class="b" aria-label="Pause recording" disabled={v.disabled} onclick={() => act("pause")}>
						<Pause size={14} aria-hidden="true" />
					</button>
				{/if}
				{#if v.showResume}
					<button class="b" aria-label="Resume recording" disabled={v.disabled} onclick={() => act("resume")}>
						<Play size={14} aria-hidden="true" />
					</button>
				{/if}
				{#if v.showStop}
					<button
						class="b stop"
						aria-label="Stop recording"
						disabled={v.disabled}
						onclick={() => act("stop")}
					>
						<Square size={13} aria-hidden="true" />
					</button>
				{/if}
				{#if v.showCancel}
					<button class="b" aria-label="Cancel recording" disabled={v.disabled} onclick={() => act("cancel")}>
						<X size={14} aria-hidden="true" />
					</button>
				{/if}
				{#if v.showOpen}
					<button class="b text" onclick={() => act("open")}>Open</button>
				{/if}
				{#if v.showClose}
					<button class="b text" onclick={() => act("close")}>Close</button>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
	}
	.hud {
		box-sizing: border-box;
		display: grid;
		place-items: center;
		width: 100vw;
		height: 100vh;
		color: var(--ca-text-primary);
		font-size: 12px;
		user-select: none;
		--line: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
	}
	.pill,
	.count {
		box-sizing: border-box;
		position: relative;
		border-radius: 18px;
		background: var(--ca-surface-raised);
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 6px 24px color-mix(in oklab, black 28%, transparent);
	}
	.pill {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 112px;
		padding: 10px 8px 8px;
	}
	.count {
		display: grid;
		place-items: center;
		width: 104px;
		height: 104px;
		border-radius: 50%;
	}
	.digit {
		font-size: 56px;
		font-weight: 600;
		line-height: 1;
		font-variant-numeric: tabular-nums;
	}
	.x {
		position: absolute;
		top: 4px;
		right: 4px;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		color: var(--ca-text-secondary);
		background: transparent;
		cursor: pointer;
	}
	.x:hover {
		color: var(--ca-text-primary);
	}
	.top {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.dot {
		width: 9px;
		height: 9px;
		border-radius: 50%;
		background: var(--ca-text-tertiary, var(--line));
	}
	.hud[data-tone="rec"] .dot,
	.hud[data-tone="warn"] .dot {
		background: var(--ca-semantic-danger, #ff453a);
	}
	.hud[data-tone="ok"] .dot {
		background: var(--ca-semantic-success);
	}
	.hud[data-tone="error"] .dot {
		background: var(--ca-semantic-warning);
	}
	.dot[data-pulse="true"] {
		animation: pulse 1.2s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.25;
		}
	}
	.timer {
		font-size: 17px;
		font-weight: 600;
		font-variant-numeric: tabular-nums;
	}
	.label,
	.left {
		max-width: 100%;
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
	.left {
		color: var(--ca-semantic-warning);
	}
	.bar {
		width: 80px;
		height: 3px;
		overflow: hidden;
		border-radius: 2px;
		background: var(--line);
	}
	.bar::after {
		content: "";
		display: block;
		width: 40%;
		height: 100%;
		background: var(--ca-text-secondary);
		animation: slide 1s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(250%);
		}
	}
	.btns {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		margin-top: 2px;
	}
	.b {
		display: grid;
		place-items: center;
		min-width: 28px;
		height: 28px;
		padding: 0;
		border: 0;
		border-radius: 14px;
		color: inherit;
		font: inherit;
		background: transparent;
		box-shadow: inset 0 0 0 1px var(--line);
		cursor: pointer;
	}
	.b.text {
		padding: 0 10px;
	}
	.b.stop {
		color: var(--ca-brand-contrast);
		background: var(--ca-semantic-danger, #ff453a);
		box-shadow: none;
	}
	.b:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.b:focus-visible,
	.x:focus-visible,
	.handle:focus-visible {
		outline: 2px solid var(--ca-accent, currentColor);
		outline-offset: 1px;
	}
	.b.primary {
		color: var(--ca-brand-contrast);
		background: var(--ca-accent, var(--ca-semantic-success));
		box-shadow: none;
	}
	.est {
		margin: 0;
		color: var(--ca-text-secondary);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
	}
	.hint {
		margin: 0;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
		font-size: 10px;
	}
	/* A note is a whole sentence about a skipped option: it wraps (the window grows for it), it never ellipsizes. */
	.note {
		max-width: 100%;
		margin: 2px 0 0;
		color: var(--ca-semantic-warning);
		font-size: 10px;
		line-height: 1.3;
		text-align: center;
		overflow-wrap: anywhere;
		white-space: normal;
	}
	.enc {
		width: 80px;
		height: 4px;
		accent-color: var(--ca-text-secondary);
	}
	.trim {
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 440px;
		padding: 12px;
		border-radius: 14px;
		background: var(--ca-surface-raised);
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 6px 24px color-mix(in oklab, black 28%, transparent);
	}
	.track {
		position: relative;
		height: 56px;
		border-radius: 6px;
		overflow: visible;
		touch-action: none;
	}
	.posters {
		display: flex;
		width: 100%;
		height: 100%;
		overflow: hidden;
		border-radius: 6px;
	}
	.posters img {
		flex: 1 1 0;
		min-width: 0;
		height: 100%;
		object-fit: cover;
		pointer-events: none;
	}
	.shade {
		position: absolute;
		top: 0;
		bottom: 0;
		background: color-mix(in oklab, black 55%, transparent);
		pointer-events: none;
	}
	.handle {
		position: absolute;
		top: -3px;
		bottom: -3px;
		width: 10px;
		margin-left: -5px;
		border-radius: 4px;
		background: var(--ca-accent, #ffd60a);
		cursor: ew-resize;
		touch-action: none;
	}
	.times {
		margin: 0;
		color: var(--ca-text-secondary);
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		text-align: center;
	}
	@media (prefers-reduced-motion: reduce) {
		.dot[data-pulse="true"],
		.bar::after {
			animation: none;
		}
	}
</style>
