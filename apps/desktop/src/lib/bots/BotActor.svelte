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
	/* Anticipation lean before first swing, overshoot + follow-through settle */
	.wave {
		animation: wave 2.8s linear infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes wave {
		/* anticipation: lean slightly opposite before swinging */
		0% {
			transform: rotate(0deg) translateY(0);
			animation-timing-function: cubic-bezier(0.3, 0, 0.7, 0);
		}
		8% {
			transform: rotate(6deg) translateY(0);
			animation-timing-function: cubic-bezier(0.2, 0, 0, 1.4);
		}
		/* swing out — spring overshoot */
		20% {
			transform: rotate(-18deg) translateY(-5px);
			animation-timing-function: cubic-bezier(0.4, 0, 0.6, 1);
		}
		32% {
			transform: rotate(10deg) translateY(-2px);
			animation-timing-function: cubic-bezier(0.4, 0, 0.6, 1);
		}
		42% {
			transform: rotate(-12deg) translateY(-4px);
			animation-timing-function: cubic-bezier(0.4, 0, 0.6, 1);
		}
		/* settle with follow-through */
		54% {
			transform: rotate(2deg) translateY(0);
			animation-timing-function: ease-in-out;
		}
		/* HOLD */
		64% {
			transform: rotate(0deg) translateY(0);
		}
		100% {
			transform: rotate(0deg) translateY(0);
		}
	}

	/* ── sip ── */
	/* Lean back (anticipation), lean forward, linger (hold), sit back (follow-through) */
	.sip {
		animation: sip 5.2s linear infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes sip {
		/* anticipation: lean back slightly */
		0% {
			transform: rotate(0deg) translateY(0);
			animation-timing-function: cubic-bezier(0.4, 0, 1, 1);
		}
		6% {
			transform: rotate(5deg) translateY(0);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		/* lean forward to sip */
		20% {
			transform: rotate(-24deg) translateY(-8px);
			animation-timing-function: cubic-bezier(0.4, 0, 0.6, 0);
		}
		/* HOLD at sip position */
		35% {
			transform: rotate(-24deg) translateY(-8px);
			animation-timing-function: cubic-bezier(0.4, 0, 1, 0.8);
		}
		/* sit back with follow-through */
		48% {
			transform: rotate(3deg) translateY(0);
			animation-timing-function: ease-in-out;
		}
		55% {
			transform: rotate(0deg) translateY(0);
		}
		/* HOLD */
		100% {
			transform: rotate(0deg) translateY(0);
		}
	}

	/* ── shake ── */
	/* Burst 0-50%, then HOLD 50-100% — no constant jiggle */
	.shake {
		animation: shake-burst 3.2s linear infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes shake-burst {
		0% {
			transform: translateX(0) rotate(0deg);
		}
		5% {
			transform: translateX(-6px) rotate(-3deg);
		}
		10% {
			transform: translateX(6px) rotate(3deg);
		}
		15% {
			transform: translateX(-6px) rotate(-3deg);
		}
		20% {
			transform: translateX(6px) rotate(3deg);
		}
		25% {
			transform: translateX(-5px) rotate(-2deg);
		}
		30% {
			transform: translateX(5px) rotate(2deg);
		}
		35% {
			transform: translateX(-4px) rotate(-1.5deg);
		}
		40% {
			transform: translateX(4px) rotate(1.5deg);
		}
		45% {
			transform: translateX(-2px) rotate(-1deg);
		}
		/* settle + HOLD */
		50% {
			transform: translateX(0) rotate(0deg);
		}
		100% {
			transform: translateX(0) rotate(0deg);
		}
	}

	/* ── cheer ── */
	/* Anticipation squash → spring jump → land squash → follow-through bounce → settle */
	.cheer {
		animation: cheer 1.1s linear infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes cheer {
		/* anticipation: sink + squash */
		0% {
			transform: translateY(0) scaleX(1) scaleY(1);
			animation-timing-function: cubic-bezier(0.6, 0, 0.4, 1);
		}
		8% {
			transform: translateY(4px) scaleX(1.12) scaleY(0.88);
			animation-timing-function: cubic-bezier(0.2, 0, 0, 1.6);
		}
		/* spring up — overshoot */
		32% {
			transform: translateY(-30px) scaleX(0.9) scaleY(1.1);
			animation-timing-function: cubic-bezier(0.8, 0, 1, 0.5);
		}
		/* land squash */
		52% {
			transform: translateY(0) scaleX(1.14) scaleY(0.86);
			animation-timing-function: cubic-bezier(0.2, 0, 0.4, 1.4);
		}
		/* follow-through: small bounce */
		68% {
			transform: translateY(-10px) scaleX(0.96) scaleY(1.04);
			animation-timing-function: ease-in-out;
		}
		/* secondary settle */
		82% {
			transform: translateY(0) scaleX(1.03) scaleY(0.97);
			animation-timing-function: ease-in-out;
		}
		/* HOLD */
		90% {
			transform: translateY(0) scaleX(1) scaleY(1);
		}
		100% {
			transform: translateY(0) scaleX(1) scaleY(1);
		}
	}

	/* ── carry ── */
	/* Weighted bob with squash on down-beat */
	.carry {
		animation: carry 2.4s linear infinite;
		animation-delay: var(--delay, 0ms);
	}
	@keyframes carry {
		0% {
			transform: rotate(-6deg) translateY(0) scaleX(1) scaleY(1);
			animation-timing-function: cubic-bezier(0.4, 0, 0.6, 1);
		}
		/* HOLD weight at top */
		28% {
			transform: rotate(-8deg) translateY(-10px) scaleX(0.97) scaleY(1.03);
			animation-timing-function: cubic-bezier(0.4, 0, 1, 1);
		}
		/* land squash — secondary motion */
		52% {
			transform: rotate(-5deg) translateY(0) scaleX(1.06) scaleY(0.94);
			animation-timing-function: cubic-bezier(0, 0, 0.4, 1);
		}
		/* float back up */
		76% {
			transform: rotate(-7deg) translateY(-6px) scaleX(0.98) scaleY(1.02);
			animation-timing-function: ease-in-out;
		}
		100% {
			transform: rotate(-6deg) translateY(0) scaleX(1) scaleY(1);
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
