<!--
  BotArtServer — step 2 onboarding illustration.
  "Each crewmate has a job": three crew members in a loose triangle,
  each with a label pill. Style: big faces, lots of empty space, gentle
  out-of-phase float. No diagrams, no boxes, no lines.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// Scale with column width, same pattern as BotArtIntro
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));

	// Three crew members: (x, y) as % of stage, base px size, float phase offset
	const crew = [
		{ seed: "art-crew-a", label: "Reviews every PR",     x: 50, y: 28, size: 110, phase: 0.0 },
		{ seed: "art-crew-b", label: "Fixes CI overnight",   x: 18, y: 60, size: 88,  phase: 1.3 },
		{ seed: "art-crew-c", label: "Writes the changelog", x: 82, y: 57, size: 96,  phase: 2.6 },
	] as const;

	// Phase accumulator drives the gentle float
	let ph = $state(reducedMotion ? Math.PI : 0);
	let paused = $state(false);

	onMount(() => {
		if (reducedMotion) return;
		let last = performance.now();
		let raf: number;
		const tick = (now: number) => {
			if (!paused) ph += ((now - last) / 1000) * (Math.PI * 0.46);
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

	function floatY(phase: number): number {
		return Math.sin(ph + phase) * 9;
	}
</script>

<div class="scene" aria-hidden="true">
	<div class="stage" bind:clientWidth={stageW}>
		{#each crew as c, i (c.seed)}
			<!--
			  Two-wrapper pattern: outer handles the JS-driven translateY float,
			  inner handles the CSS pop-in (avoids transform conflicts).
			-->
			<div
				class="float-wrap"
				style:left="{c.x}%"
				style:top="{c.y}%"
				style:transform="translate(-50%, -50%) translateY({reducedMotion
					? 0
					: floatY(c.phase)}px)"
			>
				<div class="pop-wrap" style:--pop-delay="{i * 110}ms">
					<BotFace seed={c.seed} state="idle" size={Math.round(c.size * k)} />
					<div class="pill">{c.label}</div>
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.scene {
		position: absolute;
		inset: 0;
		overflow: hidden;
	}

	.stage {
		position: absolute;
		inset: 0;
	}

	/* Outer: position + JS float, no CSS animation so there's no conflict */
	.float-wrap {
		position: absolute;
		will-change: transform;
	}

	/* Inner: pop-in animation only; no conflicting transform at rest */
	.pop-wrap {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
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
</style>
