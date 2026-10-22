<script lang="ts">
	/**
	 * Slide 3: "You chat, it works"
	 *
	 * 9-second loop:
	 *   0–1.5 s  user bubble flies in from right
	 *   1.5–3 s  reader-bot's eyes scan (state=working)
	 *   3–5 s    bot types — SpeechBubble with dots appears
	 *   5–6.5 s  bot laughs (shake action, happy eyes)
	 *   6.5–8 s  package slides from left-bot to right-bot
	 *   8–9 s    right-bot carries it away; reset
	 */
	import { onMount } from "svelte";
	import BotActor from "#lib/bots/BotActor.svelte";
	import SpeechBubble from "#lib/bots/props/SpeechBubble.svelte";
	import Package from "#lib/bots/props/Package.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	let paused = $state(false);

	onMount(() => {
		const onVisibility = () => (paused = document.hidden);
		document.addEventListener("visibilitychange", onVisibility);
		return () => document.removeEventListener("visibilitychange", onVisibility);
	});
</script>

<div class="scene" class:paused>
	<!-- User bubble — flies in from the right -->
	<div class="user-bubble" aria-hidden="true">
		<div class="bubble-body">Task: refactor auth module</div>
	</div>

	<!-- Left bot: reads → types → laughs -->
	<div class="bot-left" aria-hidden="true">
		<!-- Typing bubble — appears after reading phase -->
		<div class="typing-bubble">
			<SpeechBubble typing={true} />
		</div>
		<!-- Laugh bubble — appears after typing phase -->
		<div class="laugh-bubble">
			<SpeechBubble>
				{#snippet children()}ha ha{/snippet}
			</SpeechBubble>
		</div>

		<!-- The actor — switches action via CSS phase class on parent -->
		<div class="actor-phase">
			<!-- reading phase: state=working, idle action -->
			<div class="phase reading">
				<BotActor seed="chat-bot-a" state="working" size={84} {paused} />
			</div>
			<!-- typing phase: state=working, idle action (face bobs) -->
			<div class="phase typing-phase">
				<BotActor seed="chat-bot-a" state="working" size={84} {paused} />
			</div>
			<!-- laugh phase: state=idle, shake action, happy eyes -->
			<div class="phase laughing">
				<BotActor
					seed="chat-bot-a"
					state="idle"
					size={84}
					look={{ eyes: "happy", color: "#3d5bff", hat: "none" }}
					action="shake"
					{paused}
				/>
			</div>
		</div>
	</div>

	<!-- Package — slides from left-bot to right-bot -->
	<div class="package-transit" aria-hidden="true">
		<Package size={32} />
	</div>

	<!-- Right bot: carries the package away -->
	<div class="bot-right" aria-hidden="true">
		<div class="carry-package">
			<Package size={28} />
		</div>
		<BotActor seed="chat-bot-b" state="idle" size={72} action="carry" flipX={true} {paused} delay={300} />
	</div>
</div>

<style>
	.scene {
		position: relative;
		width: 100%;
		height: 260px;
		overflow: hidden;
	}

	/* ── user bubble ── */
	.user-bubble {
		position: absolute;
		top: 28px;
		right: 10%;
		animation: user-msg 9s ease-in-out infinite;
	}
	.bubble-body {
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		font-size: 11px;
		padding: 7px 12px;
		border-radius: 14px 14px 4px 14px;
		white-space: nowrap;
		box-shadow: 0 2px 8px rgba(79, 142, 247, 0.35);
	}
	@keyframes user-msg {
		0% {
			opacity: 0;
			transform: translateX(60px);
		}
		12%,
		88% {
			opacity: 1;
			transform: translateX(0);
		}
		98%,
		100% {
			opacity: 0;
			transform: translateX(60px);
		}
	}

	/* ── left bot container ── */
	.bot-left {
		position: absolute;
		bottom: 28px;
		left: 16%;
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	/* Typing bubble: visible from 33%–55% of 9s loop */
	.typing-bubble {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 50%;
		transform: translateX(-50%);
		animation: bubble-appear 9s ease-in-out infinite;
	}
	.laugh-bubble {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 50%;
		transform: translateX(-50%);
		animation: laugh-appear 9s ease-in-out infinite;
	}
	@keyframes bubble-appear {
		0%,
		30%,
		57%,
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
		36%,
		52% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
	}
	@keyframes laugh-appear {
		0%,
		54%,
		73%,
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
		58%,
		70% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
	}

	/* Actor phases — each fades in/out in sequence */
	.actor-phase {
		position: relative;
	}
	.phase {
		position: absolute;
		bottom: 0;
		left: 50%;
		transform: translateX(-50%);
	}
	/* First phase (reading) is the "default" — sits in flow */
	.reading {
		position: relative;
		transform: none;
	}

	.reading {
		animation: phase-read 9s ease-in-out infinite;
	}
	.typing-phase {
		animation: phase-type 9s ease-in-out infinite;
	}
	.laughing {
		animation: phase-laugh 9s ease-in-out infinite;
	}

	@keyframes phase-read {
		0%,
		16%,
		31%,
		100% {
			opacity: 1;
		}
		33%,
		72% {
			opacity: 0;
		}
		/* Show again after laugh phase resets */
	}
	@keyframes phase-type {
		0%,
		30%,
		58%,
		100% {
			opacity: 0;
		}
		34%,
		55% {
			opacity: 1;
		}
	}
	@keyframes phase-laugh {
		0%,
		56%,
		75%,
		100% {
			opacity: 0;
		}
		59%,
		72% {
			opacity: 1;
		}
	}

	/* ── package transit ── */
	.package-transit {
		position: absolute;
		/* starts at left-bot's hands, ends near right-bot */
		bottom: 60px;
		left: 30%;
		color: var(--ca-text-secondary, #64748b);
		animation: pkg-fly 9s ease-in-out infinite;
	}
	@keyframes pkg-fly {
		0%,
		70% {
			opacity: 0;
			transform: translateX(0) translateY(0);
		}
		74% {
			opacity: 1;
			transform: translateX(0) translateY(-10px);
		}
		84%,
		90% {
			opacity: 1;
			transform: translateX(140px) translateY(-4px);
		}
		95%,
		100% {
			opacity: 0;
			transform: translateX(150px) translateY(0);
		}
	}

	/* ── right bot ── */
	.bot-right {
		position: absolute;
		bottom: 28px;
		right: 10%;
		display: flex;
		flex-direction: column;
		align-items: center;
		animation: right-bot-appear 9s ease-in-out infinite;
	}
	@keyframes right-bot-appear {
		0%,
		82%,
		100% {
			opacity: 0;
			transform: translateX(24px);
		}
		88%,
		97% {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.carry-package {
		position: absolute;
		top: -8px;
		right: -8px;
		color: var(--ca-text-secondary, #64748b);
	}

	/* ── pause ── */
	.paused :global(*) {
		animation-play-state: paused !important;
	}

	@media (prefers-reduced-motion: reduce) {
		/* Show the steady-state: reading bot visible, bubble visible */
		.user-bubble {
			animation: none;
			opacity: 1;
			transform: translateX(0);
		}
		.typing-bubble {
			animation: none;
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
		.laugh-bubble {
			animation: none;
			opacity: 0;
		}
		.reading {
			animation: none;
			opacity: 1;
		}
		.typing-phase {
			animation: none;
			opacity: 0;
		}
		.laughing {
			animation: none;
			opacity: 0;
		}
		.package-transit {
			animation: none;
			opacity: 0;
		}
		.bot-right {
			animation: none;
			opacity: 1;
			transform: translateX(0);
		}
	}
</style>
