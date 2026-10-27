<script lang="ts">
	/**
	 * The Atlas globe on one canvas, in two modes that share one renderer and one visual language:
	 *   progress the globe as the scan's meter: each project found lights a sheet; a finished stack lights all of them
	 *   calm     the restrained empty state: dim globe, one tile glinting at a time
	 * requestAnimationFrame follows the display (60/120 Hz); the loop stops when hidden, unfocused, or in reduced motion.
	 */
	import { onMount } from "svelte";
	import { SHEETS as SHEET_ORDER, calm, progressFrame } from "./globe.ts";
	import { loopMode, pageIsFocused } from "./player.ts";
	import { layoutAt } from "../legibility.ts";
	import { dprQuery, planFrame, SizeWatch } from "./sizing.ts";
	import { ATLAS, paint } from "./render.ts";

	let {
		mode,
		lit = 0,
		doneAt = null,
	}: {
		mode: "progress" | "calm";
		/** Projects found so far (progress mode). */
		lit?: number;
		/** Scene-clock time the stack finished, or null (progress mode); see `clock`. */
		doneAt?: number | null;
	} = $props();

	let canvas: HTMLCanvasElement;
	let raf = 0;
	let last = 0;
	let focused = true;
	let visible = true;
	let reduced = false;
	let clock = 0;
	/** Scene-clock time the globe started easing into its backdrop place. */
	let settleFrom: number | null = null;
	const SETTLE_SECONDS = 1.4;
	let litAt: Record<number, number> = {};
	let prevLit = 0;

	/** The current scene-clock time, exported so the flow can stamp `doneAt` on the same clock the canvas uses. */
	export const now = () => clock;

	const watch = new SizeWatch();
	let retryRaf = 0;

	/**
	 * One frame: measure the box ONCE, resize the buffer if it does not match (setting width/height resets the 2D
	 * context, so the context is taken after), and paint at the CSS size. An unsized canvas (window shown later, layout
	 * not settled) paints nothing and retries on its own: a ResizeObserver is not guaranteed to fire for a box that was
	 * never non-zero, and the canvas would stay at the 300x150 default in a corner.
	 */
	function draw() {
		const r = canvas.getBoundingClientRect();
		const plan = planFrame({ cssW: r.width, cssH: r.height, dpr: window.devicePixelRatio || 1 }, canvas);
		if (plan.retry) {
			scheduleRetry();
			return;
		}
		if (plan.resize) {
			canvas.width = plan.resize.width;
			canvas.height = plan.resize.height;
		}
		const ctx = canvas.getContext("2d");
		if (!ctx) return;
		ctx.setTransform(plan.scale, 0, 0, plan.scale, 0, 0);
		const f = mode === "progress" ? progressFrame({ lit, doneAt, now: clock, litAt }) : calm(clock);
		// The globe eases into its backdrop place once, so nothing jumps; reduced motion starts there.
		settleFrom ??= reduced ? clock - SETTLE_SECONDS : clock;
		const layout = layoutAt((clock - settleFrom) / SETTLE_SECONDS);
		paint(ctx, f, plan.w, plan.h, ATLAS, { shake: reduced ? 0 : 1, layout });
		if (watch.firstSize({ w: plan.w, h: plan.h })) resume();
	}

	function scheduleRetry() {
		if (retryRaf) return;
		retryRaf = requestAnimationFrame(() => {
			retryRaf = 0;
			draw();
		});
	}

	function loop(ts: number) {
		raf = 0;
		if (loopMode({ visible, focused, reducedMotion: reduced }) !== "animate") return;
		const dt = last ? (ts - last) / 1000 : 0;
		last = ts;
		clock += Math.min(dt, 0.05);
		draw();
		raf = requestAnimationFrame(loop);
	}

	function resume() {
		last = 0;
		cancelAnimationFrame(raf);
		raf = 0;
		const m = loopMode({ visible, focused, reducedMotion: reduced });
		if (m === "static") draw();
		else if (m === "animate") raf = requestAnimationFrame(loop);
	}

	// Each new project lights a sheet and stamps when, so its flare plays from that moment.
	$effect(() => {
		if (lit > prevLit) for (const id of litNow(lit, prevLit)) litAt[id] = clock;
		prevLit = lit;
		if (!raf) draw();
	});

	function litNow(n: number, from: number): number[] {
		return SHEET_ORDER.slice(Math.floor(from), Math.floor(n));
	}

	onMount(() => {
		const media = window.matchMedia("(prefers-reduced-motion: reduce)");
		reduced = media.matches;
		draw();
		const ro = new ResizeObserver(draw);
		ro.observe(canvas);
		const io = new IntersectionObserver(([e]) => {
			visible = !!e?.isIntersecting;
			resume();
		});
		io.observe(canvas);
		const onFocus = () => {
			focused = pageIsFocused(document.hasFocus(), "__ATLAS_AGENT__" in window);
			resume();
		};
		const onMotion = () => {
			reduced = media.matches;
			resume();
		};
		// A change of screen changes the pixel ratio but not the box: re-arm the query for the new ratio and repaint.
		let dprMedia = window.matchMedia(dprQuery(window.devicePixelRatio));
		const onDpr = () => {
			dprMedia.removeEventListener("change", onDpr);
			dprMedia = window.matchMedia(dprQuery(window.devicePixelRatio));
			dprMedia.addEventListener("change", onDpr);
			draw();
		};
		dprMedia.addEventListener("change", onDpr);
		window.addEventListener("focus", onFocus);
		window.addEventListener("blur", onFocus);
		media.addEventListener("change", onMotion);
		focused = pageIsFocused(document.hasFocus(), "__ATLAS_AGENT__" in window);
		resume();
		return () => {
			cancelAnimationFrame(raf);
			cancelAnimationFrame(retryRaf);
			ro.disconnect();
			io.disconnect();
			window.removeEventListener("focus", onFocus);
			window.removeEventListener("blur", onFocus);
			media.removeEventListener("change", onMotion);
			dprMedia.removeEventListener("change", onDpr);
		};
	});
</script>

<canvas bind:this={canvas} aria-hidden="true"></canvas>

<style>
	canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
		pointer-events: none;
	}
</style>
