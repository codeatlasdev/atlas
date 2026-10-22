<script lang="ts">
	/**
	 * Slide 2: "It runs on your server"
	 *
	 * A server rack blinks on the left; a bot works at a laptop on the right.
	 * The sky slowly cycles day → dusk → night → dawn over ~9 s.
	 * Mid-loop a second laptop (the Mac) tilts its screen closed — the bot keeps going.
	 */
	import { onMount } from "svelte";
	import BotActor from "#lib/bots/BotActor.svelte";
	import ServerRack from "#lib/bots/props/ServerRack.svelte";
	import Laptop from "#lib/bots/props/Laptop.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	let paused = $state(false);
	let macOpen = $state(true);

	onMount(() => {
		if (reducedMotion) return;

		const onVisibility = () => (paused = document.hidden);
		document.addEventListener("visibilitychange", onVisibility);

		// toggle the Mac lid at 45 % and 90 % of each 9-second loop
		let frame = 0;
		const LOOP = 9000;
		let start = performance.now();

		function tick(now: number) {
			const t = ((now - start) % LOOP) / LOOP;
			macOpen = t < 0.42 || t > 0.88;
			frame = requestAnimationFrame(tick);
		}
		frame = requestAnimationFrame(tick);

		return () => {
			document.removeEventListener("visibilitychange", onVisibility);
			cancelAnimationFrame(frame);
		};
	});
</script>

<div class="scene" class:paused>
	<!-- Sky layers — opacity only for GPU-friendliness -->
	<div class="sky sky-base" aria-hidden="true"></div>
	<div class="sky sky-day" aria-hidden="true"></div>
	<div class="sky sky-dusk" aria-hidden="true"></div>
	<div class="sky sky-night" aria-hidden="true"></div>

	<!-- Ground -->
	<div class="ground" aria-hidden="true"></div>

	<!-- Left: server rack -->
	<div class="rack-wrap" aria-hidden="true">
		<ServerRack size={84} bays={4} />
	</div>

	<!-- Right: bot working at laptop -->
	<div class="bot-wrap" aria-hidden="true">
		<!-- Mac laptop (closes mid-loop) -->
		<div class="mac-wrap">
			<Laptop size={60} open={reducedMotion ? true : macOpen} />
		</div>
		<!-- The bot that keeps going -->
		<BotActor seed="server-bot" state="working" size={88} {paused} delay={200} />
		<!-- Bot's own laptop -->
		<div class="bot-laptop">
			<Laptop size={64} open={true} />
		</div>
	</div>

	<!-- Stars — visible during night phase -->
	<div class="stars" aria-hidden="true">
		{#each [{ x: 12, y: 8, d: 0 }, { x: 31, y: 5, d: 0.4 }, { x: 54, y: 10, d: 0.8 }, { x: 72, y: 4, d: 0.2 }, { x: 86, y: 12, d: 0.6 }] as s}
			<div class="star" style:left="{s.x}%" style:top="{s.y}%" style:--sd="{s.d}s"></div>
		{/each}
	</div>
</div>

<style>
	.scene {
		position: relative;
		width: 100%;
		height: 260px;
		overflow: hidden;
		border-radius: 12px;
	}

	/* ── sky ── */
	.sky {
		position: absolute;
		inset: 0;
		border-radius: inherit;
	}
	.sky-base {
		background: var(--ca-surface-sunken, #f1f5f9);
	}
	.sky-day {
		background: linear-gradient(180deg, #bfdbfe 0%, #dbeafe 50%, transparent 100%);
		animation: sky-day 9s ease-in-out infinite;
	}
	.sky-dusk {
		background: linear-gradient(180deg, #fcd34d44 0%, #f97316aa 30%, transparent 80%);
		opacity: 0;
		animation: sky-dusk 9s ease-in-out infinite;
	}
	.sky-night {
		background: linear-gradient(180deg, #0f172a 0%, #1e3a5f 60%, transparent 100%);
		opacity: 0;
		animation: sky-night 9s ease-in-out infinite;
	}

	@keyframes sky-day {
		0%,
		100% {
			opacity: 1;
		}
		45%,
		70% {
			opacity: 0;
		}
	}
	@keyframes sky-dusk {
		0%,
		20%,
		80%,
		100% {
			opacity: 0;
		}
		35%,
		60% {
			opacity: 0.9;
		}
	}
	@keyframes sky-night {
		0%,
		30%,
		90%,
		100% {
			opacity: 0;
		}
		48%,
		70% {
			opacity: 1;
		}
	}

	/* ── stars ── */
	.stars {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}
	.star {
		position: absolute;
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: #fff;
		opacity: 0;
		animation: star-twinkle 9s ease-in-out infinite;
		animation-delay: var(--sd, 0s);
	}
	@keyframes star-twinkle {
		0%,
		35%,
		85%,
		100% {
			opacity: 0;
		}
		50%,
		68% {
			opacity: 0.8;
		}
	}

	/* ── ground ── */
	.ground {
		position: absolute;
		bottom: 0;
		left: 0;
		right: 0;
		height: 40px;
		background: var(--ca-surface-overlay, rgba(0, 0, 0, 0.06));
		border-top: 1px solid var(--ca-outline-subtle, rgba(0, 0, 0, 0.1));
	}

	/* ── rack ── */
	.rack-wrap {
		position: absolute;
		bottom: 36px;
		left: 12%;
		color: var(--ca-text-secondary, #64748b);
	}

	/* ── bot zone ── */
	.bot-wrap {
		position: absolute;
		bottom: 32px;
		right: 8%;
		display: flex;
		align-items: flex-end;
		gap: 8px;
	}

	.mac-wrap {
		color: var(--ca-text-secondary, #94a3b8);
		/* The Mac slides slightly off-screen as it closes */
		animation: mac-fade 9s ease-in-out infinite;
	}
	@keyframes mac-fade {
		0%,
		38% {
			opacity: 1;
			transform: translateX(0);
		}
		48%,
		88%,
		100% {
			opacity: 0.4;
			transform: translateX(-8px);
		}
		92% {
			opacity: 1;
			transform: translateX(0);
		}
	}

	.bot-laptop {
		color: var(--ca-accent, #4f8ef7);
		margin-bottom: 4px;
	}

	/* ── pause ── */
	.paused :global(*) {
		animation-play-state: paused !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.sky-day,
		.sky-dusk,
		.sky-night {
			animation: none;
		}
		.sky-day {
			opacity: 1;
		}
		.sky-dusk {
			opacity: 0;
		}
		.sky-night {
			opacity: 0;
		}
		.star,
		.mac-wrap {
			animation: none;
		}
	}
</style>
