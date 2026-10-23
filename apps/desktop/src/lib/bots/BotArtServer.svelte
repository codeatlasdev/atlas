<!--
  BotArtServer — step 2 onboarding illustration.
  "Each crewmate has a job": composer types a role description,
  then three crewmates pop in one by one, each with a role pill
  and a trigger chip. 9 s loop. Reduced-motion: all three visible, still.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { GitPullRequest, CircleDot, Tag } from "@lucide/svelte";

	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));
	const faceSize = $derived(Math.round(84 * k));

	const PERIOD = 9000;
	let t = $state(reducedMotion ? 0.78 : 0);
	let ph = $state(0);
	let paused = $state(false);

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Composer text typewriter
	const COMPOSER_TEXT = "Review every PR in atlas-bot";
	const typedLen = $derived(
		reducedMotion ? COMPOSER_TEXT.length : Math.round(seg(t, 0.0, 0.14) * COMPOSER_TEXT.length),
	);
	const typedText = $derived(COMPOSER_TEXT.slice(0, typedLen));
	const showCursor = $derived(!reducedMotion && typedLen < COMPOSER_TEXT.length);

	// Crewmate and label visibility gates
	const show1 = $derived(reducedMotion || t >= 0.18);
	const show2 = $derived(reducedMotion || t >= 0.4);
	const show3 = $derived(reducedMotion || t >= 0.56);
	const labels1 = $derived(reducedMotion || t >= 0.27);
	const labels2 = $derived(reducedMotion || t >= 0.47);
	const labels3 = $derived(reducedMotion || t >= 0.63);

	// Gentle float, out of phase per crewmate
	function floatY(phase: number): number {
		return reducedMotion ? 0 : Math.sin(ph + phase) * 7;
	}

	onMount(() => {
		if (reducedMotion) return;
		const t0 = performance.now();
		let last = t0;
		let raf: number;
		const tick = (now: number) => {
			if (!paused) {
				t = ((now - t0) % PERIOD) / PERIOD;
				ph += ((now - last) / 1000) * (Math.PI * 0.44);
			}
			last = now;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			paused = document.hidden;
			if (!paused) last = performance.now();
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="scene" aria-hidden="true">
	<div class="stage" bind:clientWidth={stageW}>
		<!-- Composer bar: text types itself -->
		<div
			class="composer"
			style:font-size="{Math.round(13 * k)}px"
			style:padding="{Math.round(9 * k)}px {Math.round(16 * k)}px"
		>
			<span class="composer-text">{typedText}</span>{#if showCursor}<span class="cursor">▋</span>{/if}
		</div>

		<!-- Three crewmates, staggered pop-in -->
		<div class="crew-row">
			{#if show1}
				<div class="crewmate" style:transform="translateY({floatY(0.0)}px)">
					<div class="face-pop">
						<BotFace seed="art-reviewer" state="idle" size={faceSize} />
					</div>
					{#if labels1}
						<div class="label-stack labels-pop">
							<div class="pill role">Reviewer</div>
							<div class="chip">
								<GitPullRequest size={Math.round(10 * k)} strokeWidth={2} />
								<span>PR opened</span>
							</div>
						</div>
					{/if}
				</div>
			{/if}

			{#if show2}
				<div class="crewmate" style:transform="translateY({floatY(1.3)}px)">
					<div class="face-pop">
						<BotFace seed="art-fixer" state="idle" size={faceSize} />
					</div>
					{#if labels2}
						<div class="label-stack labels-pop">
							<div class="pill role">Fixes CI</div>
							<div class="chip ci">
								<CircleDot size={Math.round(10 * k)} strokeWidth={2} />
								<span>CI failed</span>
							</div>
						</div>
					{/if}
				</div>
			{/if}

			{#if show3}
				<div class="crewmate" style:transform="translateY({floatY(2.6)}px)">
					<div class="face-pop">
						<BotFace seed="art-changelog" state="idle" size={faceSize} />
					</div>
					{#if labels3}
						<div class="label-stack labels-pop">
							<div class="pill role">Changelog</div>
							<div class="chip release">
								<Tag size={Math.round(10 * k)} strokeWidth={2} />
								<span>Release</span>
							</div>
						</div>
					{/if}
				</div>
			{/if}
		</div>
	</div>
</div>

<style>
	.scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}

	.stage {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 32px;
	}

	/* Composer: mock input pill */
	.composer {
		max-width: 80%;
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.06));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.1));
		border-radius: 999px;
		font-family: inherit;
		font-weight: 400;
		color: var(--ca-text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		pointer-events: none;
		user-select: none;
		animation: pop-in 360ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@media (prefers-reduced-motion: reduce) {
		.composer {
			animation: none;
		}
	}

	.composer-text {
		color: var(--ca-text-primary);
	}

	.cursor {
		opacity: 0.6;
		animation: blink 900ms step-end infinite;
	}

	@keyframes blink {
		0%,
		100% {
			opacity: 0.6;
		}
		50% {
			opacity: 0;
		}
	}

	/* Crew row: faces side by side, centered */
	.crew-row {
		display: flex;
		align-items: flex-start;
		justify-content: center;
		gap: 24px;
	}

	.crewmate {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		will-change: transform;
	}

	/* Face pops in with spring */
	.face-pop {
		animation: pop-in 400ms 0ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	/* Labels pop in slightly after face */
	.labels-pop {
		animation: pop-in 360ms 80ms both cubic-bezier(0.34, 1.56, 0.64, 1);
	}

	@keyframes pop-in {
		from {
			opacity: 0;
			transform: scale(0.78);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.face-pop,
		.labels-pop {
			animation: none;
		}
	}

	.label-stack {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
	}

	/* Shared pill base */
	.pill {
		white-space: nowrap;
		padding: 4px 12px;
		background: var(--ca-surface-raised, rgba(255, 255, 255, 0.07));
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.1));
		border-radius: 999px;
		font-size: 12px;
		font-weight: 500;
		color: var(--ca-text-primary);
		pointer-events: none;
		user-select: none;
	}

	.pill.role {
		font-weight: 600;
	}

	/* Trigger chip: smaller, muted */
	.chip {
		display: flex;
		align-items: center;
		gap: 4px;
		white-space: nowrap;
		padding: 3px 9px;
		background: transparent;
		border: 1px solid var(--ca-border-subtle, rgba(255, 255, 255, 0.08));
		border-radius: 999px;
		font-size: 11px;
		color: var(--ca-text-secondary, rgba(255, 255, 255, 0.5));
		pointer-events: none;
		user-select: none;
	}

	.chip.ci {
		color: var(--ca-semantic-error, #f87171);
		border-color: rgba(248, 113, 113, 0.22);
	}

	.chip.release {
		color: var(--ca-semantic-success, #4ade80);
		border-color: rgba(74, 222, 128, 0.22);
	}
</style>
