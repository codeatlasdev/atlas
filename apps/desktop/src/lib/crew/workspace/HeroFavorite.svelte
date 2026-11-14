<script lang="ts">
	/**
	 * HeroFavorite — the first mate: the big face at the top of the sidebar. It fidgets now and then (one gesture
	 * every 5-11 s), paused while the window is hidden.
	 */
	import { normalizeLook } from "#lib/crew/look.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import type { Entry } from "#lib/crew/sidebar.ts";
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";

	let { hero, active, onselect }: { hero: Entry; active: boolean; onselect: () => void } = $props();

	const GESTURES = ["hop", "tilt", "wiggle"] as const;
	let gesture = $state<(typeof GESTURES)[number] | "">("");
	$effect(() => {
		let t: ReturnType<typeof setTimeout>;
		const next = () => {
			t = setTimeout(
				() => {
					if (!document.hidden) {
						gesture = GESTURES[Math.floor(Math.random() * GESTURES.length)];
						setTimeout(() => (gesture = ""), 1000);
					}
					next();
				},
				5000 + Math.random() * 6000,
			);
		};
		next();
		return () => clearTimeout(t);
	});

	const look = $derived(normalizeLook(hero.crewmate.look, hero.crewmate.id));
</script>

<div class="hero-wrap">
	<button
		class="hero-btn"
		class:active
		onclick={onselect}
		aria-label="{hero.crewmate.name} (first mate)"
		aria-pressed={active}
	>
		<div class="hero-face-wrap">
			{#if hero.crewmate.state === "working"}
				<div class="hero-dots" style:background={look.color}>
					<span class="dot"></span><span class="dot"></span><span class="dot"></span>
				</div>
			{:else}
				<!-- Every few seconds the first mate does one small gesture, so it reads as alive at a glance. -->
				<div class="hero-fidget" data-gesture={gesture}>
					<CrewmateFace
						seed={hero.crewmate.id}
						look={hero.crewmate.look}
						state={hero.crewmate.state === "needs_you" ? "needs_you" : "idle"}
						size={80}
					/>
				</div>
			{/if}
			<span class="hero-star" aria-hidden="true">★</span>
			<!-- presence badge -->
			<span
				class="hero-presence"
				class:green={hero.crewmate.state === "working"}
				class:amber={hero.crewmate.state === "needs_you"}
				aria-hidden="true"
			></span>
			{#if crew.hasUnread(hero.hostKey, hero.crewmate.id)}
				<span class="hero-unread" aria-label="Unread"></span>
			{/if}
		</div>
		<span class="hero-name">{hero.crewmate.name}</span>
	</button>
</div>

<style>
	.hero-wrap {
		flex: none;
		display: flex;
		justify-content: center;
		padding: 12px 8px 8px;
	}
	.hero-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		background: transparent;
		border: none;
		border-radius: 18px;
		padding: 14px 16px;
		cursor: default;
		transition: background 150ms;
	}
	.hero-btn:hover {
		background: var(--ca-vibrancy-fill);
	}
	.hero-btn.active {
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
	}
	.hero-face-wrap {
		position: relative;
		width: 80px;
		height: 80px;
	}
	.hero-fidget {
		transform-origin: 50% 100%;
	}
	.hero-fidget[data-gesture="hop"] {
		animation: hero-hop 700ms cubic-bezier(0.3, 0, 0.3, 1);
	}
	.hero-fidget[data-gesture="tilt"] {
		animation: hero-tilt 900ms ease-in-out;
	}
	.hero-fidget[data-gesture="wiggle"] {
		animation: hero-wiggle 800ms ease-in-out;
	}
	@keyframes hero-hop {
		0%,
		100% {
			transform: none;
		}
		20% {
			transform: scale(1.08, 0.9);
		}
		50% {
			transform: translateY(-9px) scale(0.95, 1.06);
		}
		80% {
			transform: scale(1.05, 0.95);
		}
	}
	@keyframes hero-tilt {
		0%,
		100% {
			transform: none;
		}
		35%,
		65% {
			transform: rotate(-12deg);
		}
	}
	@keyframes hero-wiggle {
		0%,
		100% {
			transform: none;
		}
		20% {
			transform: rotate(9deg);
		}
		45% {
			transform: rotate(-8deg);
		}
		70% {
			transform: rotate(4deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.hero-fidget {
			animation: none !important;
		}
	}
	.hero-dots {
		width: 80px;
		height: 80px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
	}
	.hero-star {
		position: absolute;
		top: -2px;
		right: -2px;
		font-size: 13px;
		color: var(--ca-warning, var(--ca-semantic-warning, oklch(75% 0.18 70)));
		line-height: 1;
		text-shadow: 0 0 4px rgba(0, 0, 0, 0.4);
		pointer-events: none;
	}
	.hero-presence {
		position: absolute;
		bottom: 4px;
		right: 18px;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--ca-text-disabled, #888);
		border: 2px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.hero-presence.green {
		background: var(--ca-semantic-success, #2f9e44);
	}
	.hero-presence.amber {
		background: var(--ca-semantic-warning, #e67700);
	}
	.hero-unread {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--ca-brand-main);
		border: 2px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.hero-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-primary);
		max-width: 96px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* hero dots bounce */
	.hero-dots .dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: rgba(255, 255, 255, 0.85);
		animation: dot-bounce 1.2s ease-in-out infinite;
	}
	.hero-dots .dot:nth-child(2) {
		animation-delay: 0.15s;
	}
	.hero-dots .dot:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot-bounce {
		0%,
		80%,
		100% {
			transform: scale(0.8);
			opacity: 0.5;
		}
		40% {
			transform: scale(1.1);
			opacity: 1;
		}
	}
</style>
