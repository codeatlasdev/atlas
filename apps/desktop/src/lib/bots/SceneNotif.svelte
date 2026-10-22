<script lang="ts">
	/**
	 * Slide 4: "It calls you when it needs you"
	 *
	 * 9-second loop:
	 *   0–2 s   bot bounces (state=needs_you), phone slides up
	 *   2–5 s   notification holds, Allow button pulses
	 *   5–6 s   tap animation, button press
	 *   6–8 s   bot cheers with sparkles
	 *   8–9 s   fade-reset
	 */
	import { onMount } from "svelte";
	import BotActor from "#lib/bots/BotActor.svelte";
	import PhoneNotif from "#lib/bots/props/PhoneNotif.svelte";
	import Sparkles from "#lib/bots/props/Sparkles.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	let paused = $state(false);

	onMount(() => {
		const onVisibility = () => (paused = document.hidden);
		document.addEventListener("visibilitychange", onVisibility);
		return () => document.removeEventListener("visibilitychange", onVisibility);
	});
</script>

<div class="scene" class:paused>
	<!-- Bot that bounces → cheers -->
	<div class="bot-wrap" aria-hidden="true">
		<!-- Bouncing phase (state=needs_you gives built-in hop) -->
		<div class="phase bounce-phase">
			<BotActor seed="notif-bot" state="needs_you" size={88} {paused} />
		</div>
		<!-- Cheer phase -->
		<div class="phase cheer-phase">
			<BotActor seed="notif-bot" state="idle" size={88} action="cheer" {paused} />
		</div>
	</div>

	<!-- Sparkles that appear during cheer phase -->
	<div class="sparkles-wrap" aria-hidden="true">
		<Sparkles size={120} />
	</div>

	<!-- Phone with notification -->
	<div class="phone-wrap" aria-hidden="true">
		<PhoneNotif size={76} />
	</div>

	<!-- "Allowed" confirm flash — appears after tap -->
	<div class="confirm" aria-hidden="true">Allowed</div>
</div>

<style>
	.scene {
		position: relative;
		width: 100%;
		height: 260px;
		overflow: hidden;
	}

	/* ── bot ── */
	.bot-wrap {
		position: absolute;
		bottom: 28px;
		left: 22%;
	}
	.phase {
		position: absolute;
		bottom: 0;
		left: 0;
	}
	.bounce-phase {
		position: relative; /* keeps layout flow for the wrapper */
		animation: bounce-show 9s ease-in-out infinite;
	}
	.cheer-phase {
		animation: cheer-show 9s ease-in-out infinite;
	}
	@keyframes bounce-show {
		0%,
		62%,
		100% {
			opacity: 1;
		}
		66%,
		95% {
			opacity: 0;
		}
	}
	@keyframes cheer-show {
		0%,
		62%,
		98%,
		100% {
			opacity: 0;
		}
		67%,
		94% {
			opacity: 1;
		}
	}

	/* ── sparkles ── */
	.sparkles-wrap {
		position: absolute;
		bottom: 60px;
		left: 8%;
		color: var(--ca-accent, #4f8ef7);
		animation: sparkles-show 9s ease-in-out infinite;
	}
	@keyframes sparkles-show {
		0%,
		63%,
		97%,
		100% {
			opacity: 0;
		}
		68%,
		93% {
			opacity: 1;
		}
	}

	/* ── phone ── */
	.phone-wrap {
		position: absolute;
		bottom: 20px;
		right: 16%;
		color: var(--ca-text-secondary, #64748b);
		animation: phone-appear 9s ease-in-out infinite;
	}
	@keyframes phone-appear {
		0% {
			opacity: 0;
			transform: translateY(40px);
		}
		16%,
		90% {
			opacity: 1;
			transform: translateY(0);
		}
		97%,
		100% {
			opacity: 0;
			transform: translateY(40px);
		}
	}

	/* ── confirm flash ── */
	.confirm {
		position: absolute;
		right: calc(16% + 14px);
		bottom: 110px;
		font-size: 11px;
		font-weight: 600;
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--ca-semantic-success) 12%, transparent);
		border: 1px solid color-mix(in oklab, var(--ca-semantic-success) 40%, transparent);
		border-radius: 8px;
		padding: 3px 8px;
		animation: confirm-pop 9s ease-in-out infinite;
	}
	@keyframes confirm-pop {
		0%,
		54%,
		76%,
		100% {
			opacity: 0;
			transform: scale(0.8) translateY(4px);
		}
		58%,
		72% {
			opacity: 1;
			transform: scale(1) translateY(0);
		}
	}

	/* ── pause ── */
	.paused :global(*) {
		animation-play-state: paused !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.bounce-phase {
			animation: none;
			opacity: 1;
			position: relative;
		}
		.cheer-phase {
			animation: none;
			opacity: 0;
		}
		.sparkles-wrap {
			animation: none;
			opacity: 0;
		}
		.phone-wrap {
			animation: none;
			opacity: 1;
			transform: translateY(0);
		}
		.confirm {
			animation: none;
			opacity: 0;
		}
	}
</style>
