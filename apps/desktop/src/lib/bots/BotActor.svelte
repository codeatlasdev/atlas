<script lang="ts">
	/**
	 * BotActor: a scene performer wrapper around BotFace.
	 * Adds positional CSS animations layered on top of BotFace's internal
	 * breathe/bob/hop animations. All animation uses transforms + opacity only.
	 */
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";

	export type ActorAction =
		| "idle" // no extra motion
		| "wave" // side-to-side sway with a raised-arm feel
		| "sip" // lean forward and back (drinking)
		| "shake" // rapid jitter (laughing)
		| "cheer" // big hop with arms-up energy
		| "carry"; // slight tilt + slow bob (carrying something)

	let {
		seed,
		state,
		size = 72,
		look,
		action = "idle" as ActorAction,
		flipX = false,
		paused = false,
		delay = 0,
	}: {
		seed: string;
		state: BotState;
		size?: number;
		look?: unknown;
		action?: ActorAction;
		flipX?: boolean;
		/** External pause (e.g. document.hidden from parent scene). */
		paused?: boolean;
		/** Animation start delay in ms for staggering. */
		delay?: number;
	} = $props();
</script>

<div
	class="actor"
	class:paused={paused || reducedMotion}
	class:flip={flipX}
	class:wave={action === "wave"}
	class:sip={action === "sip"}
	class:shake={action === "shake"}
	class:cheer={action === "cheer"}
	class:carry={action === "carry"}
	style:--delay="{delay}ms"
>
	<BotFace {seed} {state} {size} {look} />
</div>

<style>
	.actor {
		display: inline-flex;
		flex-direction: column;
		align-items: center;
		/* transform-origin at the feet so squash/stretch reads as weight */
		transform-origin: center bottom;
		will-change: transform;
	}

	.flip {
		transform: scaleX(-1);
	}

	/* ── wave ── */
	/* Rocks the whole body side-to-side like an enthusiastic greeting */
	.wave {
		animation: wave 2.6s ease-in-out infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes wave {
		0%,
		100% {
			transform: rotate(0deg) translateY(0);
		}
		12% {
			transform: rotate(-14deg) translateY(-4px);
		}
		24% {
			transform: rotate(8deg) translateY(-2px);
		}
		36% {
			transform: rotate(-10deg) translateY(-4px);
		}
		50% {
			transform: rotate(0deg) translateY(0);
		}
	}

	/* ── sip ── */
	/* Leans forward (toward the cup), lingers, sits back up */
	.sip {
		animation: sip 4.8s ease-in-out infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes sip {
		0%,
		100% {
			transform: rotate(0deg) translateY(0);
		}
		18%,
		32% {
			transform: rotate(-22deg) translateY(-6px);
		}
		52% {
			transform: rotate(0deg) translateY(0);
		}
	}

	/* ── shake ── */
	/* Rapid lateral vibration (laughing, excited) */
	.shake {
		animation: shake 0.32s ease-in-out infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes shake {
		0%,
		100% {
			transform: translateX(0) rotate(0deg);
		}
		25% {
			transform: translateX(-5px) rotate(-2.5deg);
		}
		75% {
			transform: translateX(5px) rotate(2.5deg);
		}
	}

	/* ── cheer ── */
	/* Energetic jump with squash on landing */
	.cheer {
		animation: cheer 0.7s cubic-bezier(0.3, 0, 0.3, 1) infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes cheer {
		0%,
		100% {
			transform: translateY(0) scaleX(1.06) scaleY(0.94);
		}
		38% {
			transform: translateY(-22px) scaleX(0.94) scaleY(1.06);
		}
		70% {
			transform: translateY(0) scaleX(1.03) scaleY(0.97);
		}
	}

	/* ── carry ── */
	/* A slight forward tilt and gentle bob — loaded with a package */
	.carry {
		animation: carry 1.6s ease-in-out infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes carry {
		0%,
		100% {
			transform: rotate(-6deg) translateY(0);
		}
		50% {
			transform: rotate(-6deg) translateY(-8px);
		}
	}

	/* ── pause / reduced-motion ── */
	.paused,
	.paused :global(*) {
		animation-play-state: paused !important;
	}
	@media (prefers-reduced-motion: reduce) {
		.actor,
		.actor :global(*) {
			animation: none !important;
			transition: none !important;
		}
	}
</style>
