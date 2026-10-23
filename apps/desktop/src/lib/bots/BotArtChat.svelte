<!--
  BotArtChat — step 3 onboarding illustration.
  "It keeps working while your Mac sleeps": an event arrives, the
  crewmate picks it up and works through steps, the laptop dims in the
  corner, then a done bubble lands. 9 s loop. Reduced-motion: steps
  all visible, face working, laptop asleep.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { GitPullRequest, Check, Moon } from "@lucide/svelte";

	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));
	const faceSize = $derived(Math.round(120 * k));

	const PERIOD = 9000;
	let t = $state(reducedMotion ? 0.7 : 0);
	let ph = $state(0);
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Face state: idle until event is picked up, then working, then idle again
	const botState = $derived<BotState>(reducedMotion || (t >= 0.14 && t < 0.68) ? "working" : "idle");

	// Event pill: PR opened
	const showEvent = $derived(reducedMotion || t >= 0.08);

	// Steps tick in one by one
	const showStep1 = $derived(reducedMotion || t >= 0.24);
	const showStep2 = $derived(reducedMotion || t >= 0.38);
	const showStep3 = $derived(reducedMotion || t >= 0.52);

	// Laptop dims in corner
	const showLaptop = $derived(reducedMotion || t >= 0.44);

	// Done bubble
	const showDone = $derived(reducedMotion || (t >= 0.66 && t < 0.96));

	// Float
	const floatY = $derived(reducedMotion ? 0 : Math.sin(ph) * 8);

	onMount(() => {
		if (reducedMotion) return;
		const t0 = performance.now();
		let last = t0;
		let raf: number;
		const tick = (now: number) => {
			if (!paused) {
				t = ((now - t0) % PERIOD) / PERIOD;
				ph += ((now - last) / 1000) * (Math.PI * 0.44);
			}
			last = now;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			paused = document.hidden;
			if (!paused) last = performance.now();
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="scene" aria-hidden="true">
	<div class="stage" bind:clientWidth={stageW}>
		<!-- Laptop sleeping in top-right corner -->
		{#if showLaptop}
			<div
				class="laptop-corner laptop-pop"
				style:right="{Math.round(28 * k)}px"
				style:top="{Math.round(28 * k)}px"
			>
				<svg
					viewBox="0 0 56 40"
					width={Math.round(56 * k)}
					height={Math.round(40 * k)}
					fill="none"
					xmlns="http://www.w3.org/2000/svg"
				>
					<!-- Screen lid -->
					<rect
						x="4"
						y="1"
						width="48"
						height="30"
						rx="3"
						fill="rgba(255,255,255,0.06)"
						stroke="rgba(255,255,255,0.14)"
						stroke-width="1.2"
					/>
					<!-- Dark screen (asleep) -->
					<rect x="7" y="4" width="42" height="24" rx="1.5" fill="rgba(0,0,0,0.55)" />
					<!-- Moon glyph on screen -->
					<Moon x="17" y="10" size={10} stroke="rgba(255,255,255,0.30)" stroke-width="1.4" />
					<!-- Base -->
					<rect
						x="0"
						y="31"
						width="56"
						height="5"
						rx="2"
						fill="rgba(255,255,255,0.08)"
						stroke="rgba(255,255,255,0.10)"
						stroke-width="1"
					/>
					<!-- Hinge -->
					<rect x="22" y="31" width="12" height="2" rx="1" fill="rgba(0,0,0,0.3)" />
				</svg>
			</div>
		{/if}

		<!-- Main content: face + pills -->
		<div class="main-col" style:transform="translateY({floatY}px)">
			<!-- Event line -->
			{#if showEvent}
				<div class="event-pill pill-pop">
					<GitPullRequest size={Math.round(11 * k)} strokeWidth={2} />
					<span>PR #115 opened</span>
				</div>
			{/if}

			<!-- Face -->
			<div class="face-wrap face-pop">
				<BotFace seed="art-worker" state={botState} size={faceSize} />
			</div>

			<!-- Steps list -->
			<div class="steps">
				{#if showStep1}
					<div class="step step-pop">
						<div class="step-check"><Check size={Math.round(9 * k)} strokeWidth={2.5} /></div>
						<span>Read 4 files</span>
					</div>
				{/if}
				{#if showStep2}
					<div class="step step-pop">
						<div class="step-check"><Check size={Math.round(9 * k)} strokeWidth={2.5} /></div>
						<span>Ran the tests</span>
					</div>
				{/if}
				{#if showStep3}
					<div class="step step-pop">
						<div class="step-check"><Check size={Math.round(9 * k)} strokeWidth={2.5} /></div>
						<span>Left 2 comments</span>
					</div>
				{/if}
			</div>

			<!-- Done bubble -->
			{#if showDone}
				<div class="done-bubble done-pop">Done · PR #115 reviewed</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}

	.stage {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* Laptop badge pinned to corner */
	.laptop-corner {
		position: absolute;
		pointer-events: none;
		user-select: none;
	}

	/* Central column: face + pills stacked */
	.main-col {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		will-change: transform;
	}

	/* ── Animations ─────────────────────────────────────────────────────── */

	@keyframes pop-in {
		from {
			opacity: 0;
			transform: scale(0.8);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	@keyframes pop-in-up {
		from {
			opacity: 0;
			transform: translateY(6px) scale(0.94);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
	}

	.face-pop {
		animation: pop-in 420ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.pill-pop {
		animation: pop-in 340ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.step-pop {
		animation: pop-in-up 300ms 0ms both cubic-bezier(0.22, 1, 0.36, 1);
	}

	.done-pop {
		animation: pop-in 320ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	.laptop-pop {
		animation: pop-in 360ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.face-pop,
		.pill-pop,
		.step-pop,
		.done-pop,
		.laptop-pop {
			animation: none;
		}
	}

	/* ── Event pill ─────────────────────────────────────────────────────── */

	.event-pill {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 14px;
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.07));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.1));
		border-radius: 999px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.55));
		white-space: nowrap;
		pointer-events: none;
		user-select: none;
	}

	/* ── Steps list ─────────────────────────────────────────────────────── */

	.steps {
		display: flex;
		flex-direction: column;
		gap: 5px;
		align-items: flex-start;
	}

	.step {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 12px;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.55));
		white-space: nowrap;
		pointer-events: none;
		user-select: none;
	}

	.step-check {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--ca-semantic-success-subtle, rgba(34, 197, 94, 0.14));
		color: var(--ca-semantic-success, #4ade80);
		flex-shrink: 0;
	}

	/* ── Done bubble ─────────────────────────────────────────────────────── */

	.done-bubble {
		padding: 6px 16px;
		background: var(--ca-semantic-success-subtle, rgba(34, 197, 94, 0.12));
		border: 1px solid var(--ca-semantic-success, rgba(74, 222, 128, 0.28));
		border-radius: 999px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-semantic-success, #4ade80);
		white-space: nowrap;
		pointer-events: none;
		user-select: none;
	}

	.face-wrap {
		pointer-events: none;
	}
</style>
