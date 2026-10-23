<!--
  BotArtChat — step 3 onboarding illustration.
  "It keeps working while your Mac sleeps": one big crewmate in "working"
  state, a ticking PR pill, and a moon pill that fades in after a short
  delay. Grok-style: minimal elements, big face, lots of empty space.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// Scale with column width
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));
	const faceSize = $derived(Math.round(140 * k));

	// Ticking elapsed time (fantasy minutes, increments every 3 s)
	let minutes = $state(0);

	// Moon pill appears after 2 s
	let moonVisible = $state(reducedMotion);

	// Gentle float
	let ph = $state(reducedMotion ? Math.PI * 0.5 : 0);
	let paused = $state(false);

	const floatY = $derived(reducedMotion ? 0 : Math.sin(ph) * 10);
	const timeLabel = $derived(minutes === 0 ? "2h" : `2h ${minutes}m`);

	onMount(() => {
		const moonTimer = setTimeout(() => (moonVisible = true), 2000);
		const tickTimer = setInterval(() => {
			if (!paused) minutes = (minutes + 1) % 60;
		}, 3000);

		if (reducedMotion) {
			return () => {
				clearTimeout(moonTimer);
				clearInterval(tickTimer);
			};
		}

		let last = performance.now();
		let raf: number;
		const tick = (now: number) => {
			if (!paused) ph += ((now - last) / 1000) * (Math.PI * 0.42);
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
			clearTimeout(moonTimer);
			clearInterval(tickTimer);
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="scene" aria-hidden="true">
	<div class="stage" bind:clientWidth={stageW}>
		<div class="crewmate" style:transform="translateY({floatY}px)">
			<div class="pop-wrap" style:--pop-delay="0ms">
				<BotFace seed="art-worker" state="working" size={faceSize} />
			</div>
			<div class="pills">
				<div class="pill pr-pill pop-delay-1">Working on PR #115 · {timeLabel}</div>
				<div class="pill moon-pill" class:visible={moonVisible}>Your Mac is asleep</div>
			</div>
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
		gap: 14px;
		will-change: transform;
	}

	/* Face pop-in */
	.pop-wrap {
		animation: pop-in 420ms var(--pop-delay, 0ms) both cubic-bezier(0.34, 1.56, 0.64, 1);
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

	.pills {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 8px;
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

	/* PR pill pops in after the face */
	.pr-pill {
		animation: pop-in 380ms 120ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.pr-pill {
			animation: none;
		}
	}

	/* Moon pill: hidden until moonVisible, then fades in */
	.moon-pill {
		opacity: 0;
		transition: opacity 600ms ease;
	}

	.moon-pill.visible {
		opacity: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.moon-pill,
		.moon-pill.visible {
			opacity: 1;
			transition: none;
		}
	}
</style>
