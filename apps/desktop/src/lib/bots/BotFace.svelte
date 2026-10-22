<script lang="ts">
	import { onMount } from "svelte";
	import { botColor, botVariant, type BotState } from "#lib/bots/botFace.ts";

	let {
		seed,
		state: botState,
		size = 36,
	}: {
		seed: string;
		state: BotState;
		size?: number;
	} = $props();

	const color = $derived(botColor(seed));
	const variant = $derived(botVariant(seed));

	// Variant adjusts the blob squish: 0=round, 1=slightly wide, 2=slightly tall, 3=wide+big
	const rx = $derived(14 + (variant === 1 || variant === 3 ? 1.5 : 0));
	const ry = $derived(12 + (variant === 2 || variant === 3 ? 1 : -0.5));

	let paused = $state(false);

	onMount(() => {
		function onVisibility() {
			paused = document.hidden;
		}
		document.addEventListener("visibilitychange", onVisibility);
		return () => document.removeEventListener("visibilitychange", onVisibility);
	});
</script>

<svg
	viewBox="0 0 40 40"
	width={size}
	height={size}
	aria-hidden="true"
	class:paused
	xmlns="http://www.w3.org/2000/svg"
>
	<!-- Blob body -->
	<g class="body body-{botState}">
		<ellipse cx="20" cy="21" {rx} {ry} fill={color} class:offline-body={botState === "offline"} />
	</g>

	<!-- Eyes -->
	{#if botState === "idle" || botState === "offline"}
		<!-- Closed arc eyes -->
		<path d="M 13 19 Q 15 17 17 19" stroke="white" stroke-width="1.5" fill="none" stroke-linecap="round" />
		<path d="M 23 19 Q 25 17 27 19" stroke="white" stroke-width="1.5" fill="none" stroke-linecap="round" />
	{:else if botState === "working"}
		<!-- Open eyes that look left/right -->
		<g class="eye-working">
			<circle cx="15" cy="19" r="2" fill="white" />
			<circle cx="15.6" cy="19.4" r="0.9" fill="#1a1a2e" />
		</g>
		<g class="eye-working">
			<circle cx="25" cy="19" r="2" fill="white" />
			<circle cx="25.6" cy="19.4" r="0.9" fill="#1a1a2e" />
		</g>
	{:else if botState === "needs_you"}
		<!-- Wide alert eyes -->
		<g class="eye-alert">
			<circle cx="15" cy="19" r="2.5" fill="white" />
			<circle cx="15" cy="19" r="1.1" fill="#1a1a2e" />
		</g>
		<g class="eye-alert">
			<circle cx="25" cy="19" r="2.5" fill="white" />
			<circle cx="25" cy="19" r="1.1" fill="#1a1a2e" />
		</g>
	{/if}

	<!-- Idle accessories: floating z particles -->
	{#if botState === "idle"}
		<text x="30" y="16" font-size="5" fill="white" font-family="serif" class="z1">z</text>
		<text x="33" y="11" font-size="4" fill="white" font-family="serif" class="z2">z</text>
		<text x="35" y="7" font-size="3" fill="white" font-family="serif" class="z3">z</text>
	{/if}

	<!-- Working accessories: coffee cup -->
	{#if botState === "working"}
		<g class="coffee-cup">
			<!-- cup body -->
			<rect x="28" y="22" width="7" height="6" rx="1" fill="#5a3e28" />
			<!-- cup handle -->
			<path
				d="M 35 23 Q 38 25 35 27"
				stroke="#5a3e28"
				stroke-width="1.2"
				fill="none"
				stroke-linecap="round"
			/>
		</g>
		<!-- steam wisps -->
		<path
			d="M 30 22 Q 29 19 31 17 Q 33 15 31 13"
			stroke="white"
			stroke-opacity="0.5"
			stroke-width="1"
			fill="none"
			stroke-linecap="round"
			class="steam1"
		/>
		<path
			d="M 33 22 Q 32 19 34 17 Q 36 15 34 13"
			stroke="white"
			stroke-opacity="0.5"
			stroke-width="1"
			fill="none"
			stroke-linecap="round"
			class="steam2"
		/>
	{/if}
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	/* ── Body animations ───────────────────────────────────── */
	.body-idle {
		animation: breathe 4s ease-in-out infinite;
		transform-origin: 20px 21px;
	}
	@keyframes breathe {
		0%,
		100% {
			transform: scale(1);
		}
		50% {
			transform: scale(1.03);
		}
	}

	.body-working {
		animation: bob 1.8s ease-in-out infinite;
		transform-origin: 20px 21px;
	}
	@keyframes bob {
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-2px);
		}
	}

	.body-needs_you {
		animation: bounce 0.6s ease-in-out infinite;
		transform-origin: 20px 21px;
	}
	@keyframes bounce {
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-4px);
		}
	}

	/* Offline: desaturated static */
	.offline-body {
		filter: grayscale(1);
		opacity: 0.5;
	}

	/* ── Eye animations ────────────────────────────────────── */
	.eye-working {
		animation: eyeshift 2s ease-in-out infinite;
		transform-origin: 20px 19px;
	}
	@keyframes eyeshift {
		0%,
		100% {
			transform: translateX(0);
		}
		40% {
			transform: translateX(-2px);
		}
		70% {
			transform: translateX(2px);
		}
	}

	.eye-alert {
		animation: eyepulse 0.6s ease-in-out infinite;
		transform-origin: 20px 19px;
	}
	@keyframes eyepulse {
		0%,
		100% {
			transform: scale(1);
		}
		50% {
			transform: scale(1.08);
		}
	}

	/* ── Z-particle animations ─────────────────────────────── */
	.z1 {
		animation: zfloat 3s 0s ease-out infinite;
		opacity: 0;
	}
	.z2 {
		animation: zfloat 3s 0.8s ease-out infinite;
		opacity: 0;
	}
	.z3 {
		animation: zfloat 3s 1.6s ease-out infinite;
		opacity: 0;
	}
	@keyframes zfloat {
		0% {
			transform: translateY(0);
			opacity: 0;
		}
		15% {
			opacity: 0.8;
		}
		100% {
			transform: translateY(-12px);
			opacity: 0;
		}
	}

	/* ── Steam animations ──────────────────────────────────── */
	.steam1 {
		animation: steamrise 2s 0s ease-in-out infinite;
	}
	.steam2 {
		animation: steamrise 2s 0.6s ease-in-out infinite;
	}
	@keyframes steamrise {
		0% {
			opacity: 0;
			transform: translateY(0);
		}
		30% {
			opacity: 0.6;
		}
		100% {
			opacity: 0;
			transform: translateY(-8px);
		}
	}

	/* ── Pause all animations ──────────────────────────────── */
	.paused * {
		animation-play-state: paused !important;
	}

	/* ── Reduced motion ────────────────────────────────────── */
	@media (prefers-reduced-motion: reduce) {
		* {
			animation: none !important;
		}
	}
</style>
