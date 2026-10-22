<script lang="ts">
	/**
	 * Slide 3: "You chat, bots collaborate"
	 *
	 * 9-second loop story (CSS keyframes only, no JS timing):
	 *   0–1.5 s   user bubble (squiggles, no text) flies from user toward Bot A
	 *   1.5–4 s   Bot A processes: SpeechBubble typing dots visible, state=working
	 *   4–6 s     Bot A laughs (shake action, happy eyes); laugh bubble (arcs, no text)
	 *   6–8.5 s   Package slides from Bot A to Bot B; Bot B switches to carry action
	 *   8.5–9 s   reset / fade
	 *
	 * Key frame: at t≈6–7 s Bot A (idle), Package (in flight), and Bot B (carry) are
	 * all simultaneously visible — the "handoff moment".
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
	<!-- Ground line -->
	<div class="ground" aria-hidden="true"></div>

	<!-- User avatar: 48 px circle with a single squiggly line (~10 % from left) -->
	<div class="user-avatar" aria-hidden="true">
		<svg width="48" height="48" viewBox="0 0 48 48" fill="none" xmlns="http://www.w3.org/2000/svg">
			<circle
				cx="24"
				cy="24"
				r="22"
				fill="var(--ca-surface-raised)"
				stroke="var(--ca-outline-subtle)"
				stroke-width="1.5"
			/>
			<!-- Squiggly line = user/person indicator, no bot face -->
			<path
				d="M11 27 Q15 20 19 27 Q23 34 27 27 Q31 20 37 27"
				stroke="var(--ca-text-tertiary)"
				stroke-width="2"
				stroke-linecap="round"
				stroke-linejoin="round"
				fill="none"
			/>
		</svg>
	</div>

	<!-- User chat bubble: 3 squiggly SVG paths, NO text — flies toward Bot A -->
	<div class="user-bubble" aria-hidden="true">
		<div class="bubble-pill">
			<svg width="52" height="22" viewBox="0 0 52 22" fill="none" aria-hidden="true">
				<path
					d="M4 6 Q8 2 12 6 Q16 10 20 6 Q24 2 28 6"
					stroke="currentColor"
					stroke-width="1.8"
					stroke-linecap="round"
					fill="none"
				/>
				<path
					d="M4 12 Q8 8 12 12 Q16 16 20 12 Q24 8 28 12"
					stroke="currentColor"
					stroke-width="1.8"
					stroke-linecap="round"
					fill="none"
				/>
				<path
					d="M4 18 Q8 14 12 18 Q16 22 20 18"
					stroke="currentColor"
					stroke-width="1.8"
					stroke-linecap="round"
					fill="none"
				/>
			</svg>
		</div>
	</div>

	<!-- Bot A (~28 % from left): working → laughing → idle -->
	<div class="bot-a" aria-hidden="true">
		<!-- Typing dots bubble: 1.5–4 s -->
		<div class="typing-bubble" aria-hidden="true">
			<SpeechBubble typing={true} />
		</div>

		<!-- Laugh bubble: 4–6 s — 3 curved arcs, NO words -->
		<div class="laugh-bubble" aria-hidden="true">
			<SpeechBubble>
				{#snippet children()}
					<svg width="46" height="22" viewBox="0 0 46 22" fill="none" aria-hidden="true">
						<path
							d="M2 18 Q8 5 14 18"
							stroke="var(--ca-text-primary)"
							stroke-width="2"
							stroke-linecap="round"
							fill="none"
						/>
						<path
							d="M16 18 Q22 5 28 18"
							stroke="var(--ca-text-primary)"
							stroke-width="2"
							stroke-linecap="round"
							fill="none"
						/>
						<path
							d="M30 18 Q38 5 46 18"
							stroke="var(--ca-text-primary)"
							stroke-width="2"
							stroke-linecap="round"
							fill="none"
						/>
					</svg>
				{/snippet}
			</SpeechBubble>
		</div>

		<!-- Phase 1: working  0–4 s -->
		<div class="bot-a-work phase">
			<BotActor seed="chat-bot-a" state="working" size={100} {paused} />
		</div>
		<!-- Phase 2: laughing  4–6 s -->
		<div class="bot-a-laugh phase">
			<BotActor seed="chat-bot-a" state="idle" size={100} look={{ eyes: "happy" }} action="shake" {paused} />
		</div>
		<!-- Phase 3: idle after laugh  6–9 s -->
		<div class="bot-a-idle phase">
			<BotActor seed="chat-bot-a" state="idle" size={100} {paused} />
		</div>
	</div>

	<!-- Package: slides from Bot A toward Bot B  6–8.5 s -->
	<div class="pkg" aria-hidden="true">
		<Package size={36} />
	</div>

	<!-- Bot B (~63 % from left): idle → carry -->
	<div class="bot-b" aria-hidden="true">
		<!-- Phase 1: idle  0–6 s -->
		<div class="bot-b-idle phase">
			<BotActor seed="chat-bot-b" state="idle" size={100} flipX={true} {paused} />
		</div>
		<!-- Phase 2: carry  6–8.5 s -->
		<div class="bot-b-carry phase">
			<BotActor seed="chat-bot-b" state="idle" size={100} flipX={true} action="carry" {paused} />
		</div>
	</div>
</div>

<style>
	/* ── layout ── */
	.scene {
		position: relative;
		width: 100%;
		height: 300px;
		overflow: hidden;
	}

	.ground {
		position: absolute;
		bottom: 0;
		left: 0;
		right: 0;
		height: 44px;
		background: var(--ca-surface-sunken);
		border-top: 1px solid var(--ca-outline-subtle);
	}

	/* ── user avatar ── */
	.user-avatar {
		position: absolute;
		bottom: 40px;
		left: 8%;
	}

	/* ── user chat bubble ── */
	/*
	 * Starts above the user avatar, then slides right to Bot A.
	 * Timing: 0–1.5 s = 0–16.7 % of the 9 s loop.
	 */
	.user-bubble {
		position: absolute;
		bottom: 112px; /* 40 px character base + 48 px avatar + 24 px gap */
		left: 8%;
		animation: user-bubble-fly 9s linear infinite;
	}
	.bubble-pill {
		background: var(--ca-accent);
		color: var(--ca-surface-base);
		padding: 7px 12px;
		border-radius: 14px 14px 4px 14px;
		box-shadow: 0 2px 10px color-mix(in srgb, var(--ca-accent) 35%, transparent);
		line-height: 0; /* collapse SVG whitespace */
	}
	@keyframes user-bubble-fly {
		/* pop in with spring */
		0% {
			opacity: 0;
			transform: translateX(0);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		4% {
			opacity: 1;
			transform: translateX(0);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		/* fly to bot — ease-out arrival */
		14% {
			opacity: 1;
			transform: translateX(155px);
		}
		/* HOLD 1 beat */
		18% {
			opacity: 1;
			transform: translateX(155px);
			animation-timing-function: ease-in;
		}
		/* fade out */
		20% {
			opacity: 0;
			transform: translateX(165px);
		}
		100% {
			opacity: 0;
			transform: translateX(165px);
		}
	}

	/* ── Bot A container ── */
	.bot-a {
		position: absolute;
		bottom: 40px;
		left: 28%;
		width: 100px;
		height: 100px;
	}

	/* Typing bubble above Bot A: 1.5–4 s = 16.7–44.4 % */
	.typing-bubble {
		position: absolute;
		bottom: calc(100% + 8px);
		left: 50%;
		animation: typing-bubble-anim 9s linear infinite;
		white-space: nowrap;
	}
	@keyframes typing-bubble-anim {
		0%,
		14% {
			opacity: 0;
			transform: translateX(-50%) scale(0.7);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		18% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
		/* HOLD */
		43% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
			animation-timing-function: ease-in;
		}
		47% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
	}

	/* Laugh bubble above Bot A: 4–6 s = 44.4–66.7 % */
	.laugh-bubble {
		position: absolute;
		bottom: calc(100% + 8px);
		left: 50%;
		animation: laugh-bubble-anim 9s linear infinite;
		white-space: nowrap;
	}
	@keyframes laugh-bubble-anim {
		0%,
		44% {
			opacity: 0;
			transform: translateX(-50%) scale(0.7);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		48% {
			opacity: 1;
			transform: translateX(-50%) scale(1.05);
			animation-timing-function: ease-in-out;
		}
		50% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
		/* HOLD */
		64% {
			opacity: 1;
			animation-timing-function: ease-in;
		}
		68% {
			opacity: 0;
			transform: translateX(-50%) scale(0.8);
		}
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.8);
		}
	}

	/* All phase divs stack in place inside their bot container */
	.phase {
		position: absolute;
		bottom: 0;
		left: 0;
	}

	/* Bot A phase 1: working  0–4 s = 0–44.4 % */
	.bot-a-work {
		animation: bot-a-work-anim 9s linear infinite;
	}
	@keyframes bot-a-work-anim {
		0%,
		42% {
			opacity: 1;
		}
		46%,
		100% {
			opacity: 0;
		}
	}

	/* Bot A phase 2: laughing  4–6 s = 44.4–66.7 % */
	.bot-a-laugh {
		animation: bot-a-laugh-anim 9s linear infinite;
	}
	@keyframes bot-a-laugh-anim {
		0%,
		43% {
			opacity: 0;
		}
		47%,
		64% {
			opacity: 1;
		}
		68%,
		100% {
			opacity: 0;
		}
	}

	/* Bot A phase 3: idle  6–9 s = 66.7–100 % */
	.bot-a-idle {
		animation: bot-a-idle-anim 9s linear infinite;
	}
	@keyframes bot-a-idle-anim {
		0%,
		64% {
			opacity: 0;
		}
		68%,
		100% {
			opacity: 1;
		}
	}

	/* ── Package ── */
	/*
	 * Starts near Bot A's right (left: 33 %), slides ~220 px right toward Bot B.
	 * Timing: 6–8.5 s = 66.7–94.4 %.
	 */
	.pkg {
		position: absolute;
		bottom: 72px; /* roughly hand height above ground */
		left: 33%;
		color: var(--ca-text-secondary);
		animation: pkg-slide 9s linear infinite;
	}
	@keyframes pkg-slide {
		0%,
		64% {
			opacity: 0;
			transform: translateX(0) translateY(0);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		/* pop up with spring (anticipation arc) */
		67% {
			opacity: 1;
			transform: translateX(0) translateY(-12px);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		/* fly across — ease-out deceleration */
		86% {
			opacity: 1;
			transform: translateX(200px) translateY(-4px);
			animation-timing-function: cubic-bezier(0.4, 0, 1, 1);
		}
		/* land squash (handled by Bot B carry) */
		91% {
			opacity: 1;
			transform: translateX(215px) translateY(0);
		}
		/* HOLD briefly then fade */
		94% {
			opacity: 1;
			animation-timing-function: ease-in;
		}
		97% {
			opacity: 0;
			transform: translateX(220px) translateY(0);
		}
		100% {
			opacity: 0;
			transform: translateX(220px) translateY(0);
		}
	}

	/* ── Bot B container ── */
	.bot-b {
		position: absolute;
		bottom: 40px;
		left: 63%;
		width: 100px;
		height: 100px;
	}

	/* Bot B phase 1: idle  0–6 s = 0–66.7 % */
	.bot-b-idle {
		animation: bot-b-idle-anim 9s linear infinite;
	}
	@keyframes bot-b-idle-anim {
		0%,
		64% {
			opacity: 1;
		}
		68%,
		100% {
			opacity: 0;
		}
	}

	/* Bot B phase 2: carry  6–8.5 s = 66.7–94.4 % */
	.bot-b-carry {
		animation: bot-b-carry-anim 9s linear infinite;
	}
	@keyframes bot-b-carry-anim {
		0%,
		64% {
			opacity: 0;
		}
		68%,
		93% {
			opacity: 1;
		}
		96%,
		100% {
			opacity: 0;
		}
	}

	/* ── pause ── */
	.paused :global(*) {
		animation-play-state: paused !important;
	}

	/*
	 * Reduced-motion steady state:
	 * Both bots visible, typing bubble showing, package hidden.
	 */
	@media (prefers-reduced-motion: reduce) {
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
		.bot-a-work {
			animation: none;
			opacity: 1;
		}
		.bot-a-laugh {
			animation: none;
			opacity: 0;
		}
		.bot-a-idle {
			animation: none;
			opacity: 0;
		}
		.pkg {
			animation: none;
			opacity: 0;
		}
		.bot-b-idle {
			animation: none;
			opacity: 1;
		}
		.bot-b-carry {
			animation: none;
			opacity: 0;
		}
	}
</style>
