<script lang="ts">
	/**
	 * Slide 4: "It calls you when it needs you"
	 *
	 * 9-second loop:
	 *   0–2.5 s   bot bounces (state=needs_you), phone slides up from below
	 *   2.5–5 s   notification card visible, Allow button pulses
	 *   5–6.5 s   tap ring fires on Allow pill
	 *   6.5–8.5 s bot cheers with sparkles burst
	 *   8.5–9 s   reset
	 */
	import { onMount } from "svelte";
	import BotActor from "#lib/bots/BotActor.svelte";
	import PhoneNotif from "#lib/bots/props/PhoneNotif.svelte";
	import Sparkles from "#lib/bots/props/Sparkles.svelte";

	let paused = $state(false);

	onMount(() => {
		const onVisibility = () => (paused = document.hidden);
		document.addEventListener("visibilitychange", onVisibility);
		return () => document.removeEventListener("visibilitychange", onVisibility);
	});
</script>

<div class="scene" class:paused>
	<!-- Ground line -->
	<div class="ground" aria-hidden="true"></div>

	<!-- Phone with notification — left-centre of stage -->
	<div class="phone-wrap" aria-hidden="true">
		<PhoneNotif size={84} />
	</div>

	<!-- Bot and sparkles — right-centre, close to phone -->
	<div class="bot-wrap" aria-hidden="true">
		<!-- Bounce phase (0–65 %) -->
		<div class="phase bounce-phase">
			<BotActor seed="notif-bot" state="needs_you" size={96} {paused} />
		</div>
		<!-- Cheer phase (65–95 %) -->
		<div class="phase cheer-phase">
			<BotActor seed="notif-bot" state="idle" action="cheer" size={96} {paused} />
		</div>
	</div>

	<!-- Sparkles burst around the bot during cheer phase -->
	<div class="sparkles-wrap" aria-hidden="true">
		<Sparkles size={100} />
	</div>

	<!-- "Allowed" confirm flash — appears after tap -->
	<div class="confirm" aria-hidden="true">Allowed ✓</div>
</div>

<style>
	.scene {
		position: relative;
		width: 100%;
		height: 300px;
		overflow: hidden;
	}

	/* Ground line */
	.ground {
		position: absolute;
		bottom: 28px;
		left: 5%;
		right: 5%;
		height: 1px;
		background: color-mix(in oklab, currentColor 12%, transparent);
	}

	/* ── phone ── (positioned at ~35 % from left) */
	.phone-wrap {
		position: absolute;
		bottom: 30px;
		left: 32%;
		color: var(--ca-text-secondary);
		animation: phone-appear 9s ease-in-out infinite;
	}
	@keyframes phone-appear {
		0% {
			opacity: 0;
			transform: translateY(44px);
		}
		20%,
		93% {
			opacity: 1;
			transform: translateY(0);
		}
		97%,
		100% {
			opacity: 0;
			transform: translateY(44px);
		}
	}

	/* ── bot ── (positioned at ~58 % from left — close to phone) */
	.bot-wrap {
		position: absolute;
		bottom: 30px;
		left: 56%;
	}
	.phase {
		position: absolute;
		bottom: 0;
		left: 0;
	}
	.bounce-phase {
		position: relative; /* anchors wrapper height */
		animation: bounce-show 9s ease-in-out infinite;
	}
	.cheer-phase {
		animation: cheer-show 9s ease-in-out infinite;
	}
	@keyframes bounce-show {
		0%,
		63% {
			opacity: 1;
		}
		67%,
		100% {
			opacity: 0;
		}
	}
	@keyframes cheer-show {
		0%,
		65%,
		97%,
		100% {
			opacity: 0;
		}
		69%,
		93% {
			opacity: 1;
		}
	}

	/* ── sparkles ── (overlap the bot from above) */
	.sparkles-wrap {
		position: absolute;
		bottom: 60px;
		left: 56%;
		z-index: 1;
		color: var(--ca-accent);
		animation: sparkles-show 9s ease-in-out infinite;
	}
	@keyframes sparkles-show {
		0%,
		66%,
		96%,
		100% {
			opacity: 0;
		}
		71%,
		93% {
			opacity: 1;
		}
	}

	/* ── confirm flash ── */
	.confirm {
		position: absolute;
		left: calc(32% + 8px);
		bottom: 140px;
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
