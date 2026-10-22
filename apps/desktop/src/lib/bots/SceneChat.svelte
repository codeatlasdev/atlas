<script lang="ts">
	/**
	 * Slide 3: "You chat, bots collaborate"
	 *
	 * 9-second loop story (CSS keyframes only, no JS timing):
	 *   0–1.5 s   user bubble (squiggles, no text) flies from right toward Bot A
	 *   1.5–4 s   Bot A processes: SpeechBubble typing dots visible, state=working
	 *   4–6 s     Bot A laughs: laugh bubble (arcs, no text) visible
	 *   6–8.5 s   Package slides from Bot A to Bot B
	 *   8.5–9 s   reset / fade
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
	<!-- Ground -->
	<div class="ground" aria-hidden="true"></div>

	<!-- Bot A: always visible, center-left stage -->
	<div class="bot-a" aria-hidden="true">
		<!-- typing bubble: 17-46% of loop -->
		<div class="typing-bubble">
			<SpeechBubble typing={true} size={1.1} />
		</div>
		<!-- laugh bubble: 47-65% of loop -->
		<div class="laugh-bubble">
			<SpeechBubble>
				{#snippet children()}
					<!-- three laugh arcs (no text) -->
					<svg viewBox="0 0 36 14" width="36" height="14" aria-hidden="true">
						<path
							d="M2 10 Q6 2 10 10"
							stroke="currentColor"
							stroke-width="2"
							fill="none"
							stroke-linecap="round"
						/>
						<path
							d="M14 10 Q18 2 22 10"
							stroke="currentColor"
							stroke-width="2"
							fill="none"
							stroke-linecap="round"
						/>
						<path
							d="M26 10 Q30 2 34 10"
							stroke="currentColor"
							stroke-width="2"
							fill="none"
							stroke-linecap="round"
						/>
					</svg>
				{/snippet}
			</SpeechBubble>
		</div>
		<!-- Bot A working/idle (always on stage) -->
		<BotActor seed="chat-bot-a" state="working" size={112} {paused} />
	</div>

	<!-- User message bubble: flies from right, 0-17% of loop -->
	<div class="user-bubble" aria-hidden="true">
		<div class="bubble-pill">
			<!-- three squiggly lines — no text -->
			<svg viewBox="0 0 56 18" width="56" height="18" aria-hidden="true">
				<path
					d="M4 6 Q10 2 16 6 Q22 10 28 6 Q34 2 40 6 Q46 10 52 6"
					stroke="currentColor"
					stroke-width="2.5"
					fill="none"
					stroke-linecap="round"
				/>
				<path
					d="M4 12 Q8 8 12 12 Q16 16 20 12 Q24 8 28 12"
					stroke="currentColor"
					stroke-width="2"
					fill="none"
					stroke-linecap="round"
					opacity=".6"
				/>
			</svg>
		</div>
	</div>

	<!-- Package: accent color, 52px, flies A→B during 66-91% of loop -->
	<div class="pkg" aria-hidden="true">
		<Package size={52} />
	</div>

	<!-- Bot B: always visible, center-right stage -->
	<div class="bot-b" aria-hidden="true">
		<BotActor seed="chat-bot-b" state="idle" size={112} flipX={true} {paused} delay={300} />
	</div>
</div>

<style>
	/* ── scene: NO background, NO border-radius — sits free on page ── */
	.scene {
		position: relative;
		width: 100%;
		height: 260px;
		overflow: visible;
	}

	.ground {
		position: absolute;
		bottom: 0;
		left: 0;
		right: 0;
		height: 1px;
		background: var(--ca-outline-subtle);
		opacity: 0.4;
	}

	/* ── Bot A: center-left, always visible ── */
	.bot-a {
		position: absolute;
		bottom: 8px;
		left: 22%;
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	/* Typing bubble above Bot A */
	.typing-bubble {
		position: absolute;
		bottom: calc(100% + 10px);
		left: 50%;
		animation: typing-bubble-anim 9s linear infinite;
	}
	@keyframes typing-bubble-anim {
		/* visible from the start so scene reads immediately */
		0%,
		44% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
			animation-timing-function: ease-in;
		}
		48% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
	}

	/* Laugh bubble above Bot A */
	.laugh-bubble {
		position: absolute;
		bottom: calc(100% + 10px);
		left: 50%;
		animation: laugh-bubble-anim 9s linear infinite;
	}
	@keyframes laugh-bubble-anim {
		0%,
		45% {
			opacity: 0;
			transform: translateX(-50%) scale(0.7);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		49% {
			opacity: 1;
			transform: translateX(-50%) scale(1.05);
			animation-timing-function: ease-in-out;
		}
		51% {
			opacity: 1;
			transform: translateX(-50%) scale(1);
		}
		51%,
		63% {
			opacity: 1;
			animation-timing-function: ease-in;
		}
		67% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
		100% {
			opacity: 0;
			transform: translateX(-50%) scale(0.85);
		}
	}

	/* ── User bubble: flies in from right ── */
	.user-bubble {
		position: absolute;
		bottom: 148px;
		right: 10%;
		animation: user-bubble-fly 9s linear infinite;
	}
	.bubble-pill {
		background: var(--ca-accent);
		color: var(--ca-surface-base);
		padding: 8px 14px;
		border-radius: 16px 16px 4px 16px;
		box-shadow: 0 3px 14px color-mix(in srgb, var(--ca-accent) 45%, transparent);
	}
	@keyframes user-bubble-fly {
		0% {
			opacity: 0;
			transform: translateX(60px);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		4% {
			opacity: 1;
			transform: translateX(60px);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		13% {
			opacity: 1;
			transform: translateX(0);
		}
		13%,
		17% {
			opacity: 1;
			transform: translateX(0);
			animation-timing-function: ease-in;
		}
		20% {
			opacity: 0;
			transform: translateX(-8px);
		}
		100% {
			opacity: 0;
			transform: translateX(-8px);
		}
	}

	/* ── Package: ACCENT colored, 52px, flies A→B ── */
	.pkg {
		position: absolute;
		bottom: 72px;
		left: 28%;
		color: var(--ca-accent);
		animation: pkg-fly 9s linear infinite;
	}
	@keyframes pkg-fly {
		0%,
		63% {
			opacity: 0;
			transform: translateX(0) translateY(0);
			animation-timing-function: cubic-bezier(0.34, 1.56, 0.64, 1);
		}
		67% {
			opacity: 1;
			transform: translateX(0) translateY(-16px);
			animation-timing-function: cubic-bezier(0, 0, 0.2, 1);
		}
		84% {
			opacity: 1;
			transform: translateX(220px) translateY(-8px);
			animation-timing-function: cubic-bezier(0.4, 0, 1, 1);
		}
		90% {
			opacity: 1;
			transform: translateX(240px) translateY(0);
		}
		90%,
		93% {
			opacity: 1;
			animation-timing-function: ease-in;
		}
		96% {
			opacity: 0;
			transform: translateX(245px) translateY(0);
		}
		100% {
			opacity: 0;
			transform: translateX(245px) translateY(0);
		}
	}

	/* ── Bot B: center-right, always visible ── */
	.bot-b {
		position: absolute;
		bottom: 8px;
		left: 62%;
	}

	/* ── pause ── */
	.paused :global(*) {
		animation-play-state: paused !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.user-bubble {
			animation: none;
			opacity: 0;
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
		.pkg {
			animation: none;
			opacity: 0;
		}
	}
</style>
