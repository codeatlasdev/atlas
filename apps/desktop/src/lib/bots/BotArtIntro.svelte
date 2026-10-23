<!--
  BotArtIntro — step 1 illustration.
  Centre-anchor approach: .group sits at exactly (50%, 50%) of the right
  panel; every face is a pixel offset from that anchor, so the cluster
  is guaranteed to sit in the middle of the column.
-->
<script lang="ts">
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// dx / dy: pixel offset from the panel centre. Positive dx = right, positive dy = down.
	const FACES = [
		{ seed: "ob-a", size: 140, dx: 0, dy: 0, depth: 0, phase: 0.0, state: "idle" as const, track: true },
		{ seed: "ob-b", size: 104, dx: -88, dy: 62, depth: 1, phase: 0.9, state: "idle" as const, track: false },
		{ seed: "ob-c", size: 94, dx: 88, dy: 62, depth: 1, phase: 1.7, state: "idle" as const, track: false },
		{ seed: "ob-d", size: 80, dx: -55, dy: -96, depth: 2, phase: 0.5, state: "idle" as const, track: false },
		{ seed: "ob-e", size: 74, dx: 55, dy: -102, depth: 2, phase: 1.3, state: "idle" as const, track: false },
		{
			seed: "ob-f",
			size: 68,
			dx: -152,
			dy: 6,
			depth: 3,
			phase: 2.1,
			state: "offline" as const,
			track: false,
		},
	] as const;

	const DEPTH = [
		{ scale: 1.0, opacity: 1.0, blur: 0.0, z: 6 },
		{ scale: 0.88, opacity: 0.9, blur: 0.0, z: 4 },
		{ scale: 0.76, opacity: 0.76, blur: 0.6, z: 2 },
		{ scale: 0.66, opacity: 0.58, blur: 1.0, z: 1 },
	];
</script>

<div class="art-cluster" aria-hidden="true">
	<!-- This div is at exactly (50%, 50%) of the art-zone. -->
	<div class="group">
		{#each FACES as f (f.seed)}
			{@const d = DEPTH[f.depth]}
			<div
				class="face-wrap"
				style:z-index={d.z}
				style:--dx="{f.dx}px"
				style:--dy="{f.dy}px"
				style:--sc={d.scale}
				style:--op={d.opacity}
				style:--bl="{d.blur}px"
			>
				<div class="face-bob" class:reduced={reducedMotion} style:--phase="{f.phase}s">
					<BotFace seed={f.seed} state={f.state} size={f.size} track={f.track} />
				</div>
			</div>
		{/each}
	</div>
</div>

<style>
	.art-cluster {
		position: absolute;
		inset: 0;
		filter: drop-shadow(0 28px 40px rgba(0, 0, 0, 0.55));
	}

	/* pinned to 50 / 50 of the art-zone */
	.group {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 0;
		height: 0;
	}

	.face-wrap {
		position: absolute;
		top: 0;
		left: 0;
		/* translate so face centre lands at (dx, dy) from group anchor */
		transform: translate(calc(-50% + var(--dx)), calc(-50% + var(--dy))) scale(var(--sc));
		opacity: var(--op);
		filter: blur(var(--bl));
	}

	.face-bob {
		animation: bob 5.2s ease-in-out infinite;
		animation-delay: var(--phase, 0s);
	}

	.face-bob.reduced {
		animation: none;
	}

	@keyframes bob {
		0%,
		100% {
			transform: translateY(0) rotate(-1.5deg);
		}
		50% {
			transform: translateY(-11px) rotate(1.5deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.face-bob {
			animation: none;
		}
	}
</style>
