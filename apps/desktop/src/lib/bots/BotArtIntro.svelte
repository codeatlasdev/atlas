<!--
  BotArtMascots — step 1 illustration.
  A constellation of BotFaces: several sizes, drifting with a gentle bob,
  one asleep, one glancing at the cursor. No mascots elsewhere.
-->
<script lang="ts">
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	const FACES = [
		// large, centre-top — tracks cursor (glances)
		{ seed: "ob-a", size: 110, x: 50, y: 14, phase: 0, state: "idle" as const, track: true },
		// medium, left
		{ seed: "ob-b", size: 80, x: 11, y: 34, phase: 0.9, state: "idle" as const, track: false },
		// small, top-right
		{ seed: "ob-c", size: 58, x: 80, y: 10, phase: 1.6, state: "idle" as const, track: false },
		// large, asleep — bottom-left
		{ seed: "ob-d", size: 92, x: 9, y: 64, phase: 1.1, state: "offline" as const, track: false },
		// medium, bottom-right
		{ seed: "ob-e", size: 74, x: 78, y: 62, phase: 2.2, state: "idle" as const, track: false },
		// tiny, centre
		{ seed: "ob-f", size: 50, x: 49, y: 56, phase: 0.4, state: "idle" as const, track: false },
	] as const;
</script>

<div class="art-mascots" aria-hidden="true">
	{#each FACES as f (f.seed)}
		<div
			class="face-wrap"
			class:reduced={reducedMotion}
			style:left="{f.x}%"
			style:top="{f.y}%"
			style:--phase="{f.phase}s"
		>
			<BotFace seed={f.seed} state={f.state} size={f.size} track={f.track} />
		</div>
	{/each}
</div>

<style>
	.art-mascots {
		position: relative;
		width: 100%;
		height: 100%;
		overflow: hidden;
	}

	.face-wrap {
		position: absolute;
		transform: translate(-50%, -50%);
		animation: bob 5.2s ease-in-out infinite;
		animation-delay: var(--phase, 0s);
	}

	.face-wrap.reduced {
		animation: none;
	}

	@keyframes bob {
		0%,
		100% {
			transform: translate(-50%, -50%) rotate(-2deg);
		}
		50% {
			transform: translate(-50%, calc(-50% - 13px)) rotate(2deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.face-wrap {
			animation: none;
		}
	}
</style>
