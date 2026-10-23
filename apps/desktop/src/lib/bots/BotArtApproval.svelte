<!--
  BotArtApproval — step 4 onboarding illustration.
  "It asks before anything risky": one crewmate in needs_you state,
  a request pill, Deny/Allow actions. Allow presses, face recovers to
  working, "Allowed by you" appears. Loops every ~5.5 s.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// Scale with column width
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));
	const faceSize = $derived(Math.round(130 * k));

	// Animation clock: 0→1 looping every PERIOD ms
	const PERIOD = 5500;
	let t = $state(reducedMotion ? 0.3 : 0);
	let ph = $state(0); // secondary phase for float bob
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Derived states from t
	// 0.00–0.08: face pops in
	// 0.08–0.54: waiting — needs_you, Deny/Allow visible
	// 0.54–0.68: Allow presses (scale down) → face transitions
	// 0.68–0.92: face "working", "Allowed by you" pill shows
	// 0.92–1.00: short pause before loop reset
	const botState = $derived<BotState>(t >= 0.68 && t < 0.92 ? "working" : "needs_you");
	const showActions = $derived(t >= 0.08 && t < 0.68);
	const allowPressed = $derived(t >= 0.54 && t < 0.68);
	const allowScale = $derived(allowPressed ? 1 - seg(t, 0.54, 0.62) * 0.14 : 1);
	const showAllowed = $derived(t >= 0.68 && t < 0.92);

	// Gentle bob, independent of loop clock
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
		<div class="crewmate" style:transform="translateY({floatY}px)">
			<!-- Face: pops in on mount, state changes with animation clock -->
			<div class="pop-wrap">
				<BotFace seed="art-guard" state={botState} size={faceSize} />
			</div>

			<!-- Always-visible request pill -->
			<div class="pill request-pill">Wants to run git push</div>

			<!-- Deny / Allow actions -->
			{#if showActions}
				<div class="action-row">
					<div class="pill action-pill deny">Deny</div>
					<div
						class="pill action-pill allow"
						style:transform="scale({allowScale})"
					>Allow</div>
				</div>
			{/if}

			<!-- Allowed confirmation -->
			{#if showAllowed}
				<div class="pill allowed-pill">Allowed by you</div>
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

	.crewmate {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		will-change: transform;
	}

	/* Face pop-in on mount */
	.pop-wrap {
		animation: pop-in 420ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

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

	@media (prefers-reduced-motion: reduce) {
		.pop-wrap {
			animation: none;
		}
	}

	.pill {
		white-space: nowrap;
		padding: 5px 14px;
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.07));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.10));
		border-radius: 999px;
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		pointer-events: none;
		user-select: none;
	}

	/* Request pill: pops in after the face */
	.request-pill {
		animation: pop-in 380ms 100ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.request-pill {
			animation: none;
		}
	}

	/* Action row with Deny + Allow */
	.action-row {
		display: flex;
		gap: 8px;
		animation: pop-in 320ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.action-row {
			animation: none;
		}
	}

	.action-pill {
		padding: 5px 18px;
		font-size: 13px;
		font-weight: 600;
		will-change: transform;
	}

	.action-pill.deny {
		color: var(--ca-text-secondary);
	}

	.action-pill.allow {
		background: var(--ca-accent, #4f8ef7);
		border-color: transparent;
		color: #fff;
		transform-origin: center;
	}

	/* "Allowed by you" pill pops in */
	.allowed-pill {
		animation: pop-in 320ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
		background: var(--ca-semantic-success-subtle, rgba(34, 197, 94, 0.12));
		border-color: var(--ca-semantic-success, rgba(34, 197, 94, 0.3));
		color: var(--ca-semantic-success, #22c55e);
	}

	@media (prefers-reduced-motion: reduce) {
		.allowed-pill {
			animation: none;
		}
	}
</style>
