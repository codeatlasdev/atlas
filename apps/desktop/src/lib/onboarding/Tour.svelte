<script lang="ts">
	import { Button, Kbd } from "@codeatlasdev/ui";
	import { onMount, tick } from "svelte";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { onboarding } from "./state.svelte.ts";
	import {
		TOUR_STEPS,
		nextIndex,
		placeCard,
		prevIndex,
		reanchor,
		visibleSteps,
		type Box,
		type TourStep,
	} from "./tour.ts";

	const PAD = 6;
	const CARD_W = 300;

	let steps = $state<TourStep[]>([]);
	let index = $state(0);
	let box = $state<Box | null>(null);
	let view = $state({ w: window.innerWidth, h: window.innerHeight });
	let card = $state<HTMLDivElement>();
	let cardH = $state(150);

	let step = $derived(steps[index]);
	let pos = $derived(box && step ? placeCard(box, { w: CARD_W, h: cardH }, view, step.side) : null);

	const anchor = (name: string) => document.querySelector<HTMLElement>(`[data-tour="${name}"]`);
	/** Present, rendered and not collapsed. */
	const usable = (name: string) => {
		const r = anchor(name)?.getBoundingClientRect();
		return !!r && r.width > 0 && r.height > 0;
	};

	function measure() {
		view = { w: window.innerWidth, h: window.innerHeight };
		const current = step?.id;
		const next = visibleSteps(TOUR_STEPS, usable);
		steps = next;
		index = reanchor(next, current, index);
		const r = next[index] && anchor(next[index].anchor)?.getBoundingClientRect();
		box = r ? { x: r.x, y: r.y, w: r.width, h: r.height } : null;
		if (card) cardH = card.offsetHeight;
	}

	async function go(i: number | null) {
		if (i === null) return void onboarding.endTour();
		index = i;
		anchor(steps[i].anchor)?.scrollIntoView({ block: "nearest", inline: "nearest" });
		await tick();
		measure();
	}

	const next = () => go(nextIndex(index, steps.length));
	const back = () => go(prevIndex(index));
	const skip = () => void onboarding.endTour();

	function onkeydown(e: KeyboardEvent) {
		if (e.metaKey || e.ctrlKey || e.altKey) return;
		const handled =
			e.key === "ArrowRight" || e.key === "Enter" || e.key === "ArrowLeft" || e.key === "Escape";
		if (!handled) return;
		e.preventDefault();
		e.stopPropagation();
		if (e.key === "Escape") skip();
		else if (e.key === "ArrowLeft") void back();
		else void next();
	}

	onMount(() => {
		measure();
		// No anchor on screen at all (e.g. no project selected): nothing to show.
		if (!steps.length) skip();
		const ro = new ResizeObserver(measure);
		ro.observe(document.documentElement);
		// Docks and sidebars animate: re-measure while the layout settles.
		const poll = setInterval(measure, 250);
		window.addEventListener("resize", measure);
		return () => {
			ro.disconnect();
			clearInterval(poll);
			window.removeEventListener("resize", measure);
		};
	});

	$effect(() => {
		// The card height changes with the step's copy: re-measure when the step changes.
		if (step?.id && card) cardH = card.offsetHeight;
	});
</script>

<svelte:window onkeydowncapture={onkeydown} />

{#if step && box && pos}
	<div class="tour" role="dialog" aria-modal="true" aria-label="Guided tour">
		<!-- Clicks outside the spotlight are swallowed so the tour can't be left half-done by accident. -->
		<button class="scrim" tabindex="-1" aria-label="Skip tour" onclick={skip}></button>
		<div
			class="spot"
			style:left="{box.x - PAD}px"
			style:top="{box.y - PAD}px"
			style:width="{box.w + PAD * 2}px"
			style:height="{box.h + PAD * 2}px"
		></div>
		<div
			bind:this={card}
			class="card"
			data-side={pos.side}
			style:left="{pos.x}px"
			style:top="{pos.y}px"
			style:width="{CARD_W}px"
		>
			<div class="step">{index + 1} of {steps.length}</div>
			<h3>{step.title}</h3>
			<p>{step.body}</p>
			{#if step.id === "palette"}
				<p class="hint">
					Press <Kbd>{shortcutFor("atlas::Jump") ?? "⌘K"}</Kbd> any time.
				</p>
			{/if}
			<div class="foot">
				<div class="dots" aria-hidden="true">
					{#each steps as s, i (s.id)}<span class:on={i === index} class:seen={i < index}></span>{/each}
				</div>
				<button class="skip" onclick={skip}>Skip</button>
				<Button variant="ghost" size="sm" disabled={index === 0} onclick={back}>Back</Button>
				<Button size="sm" onclick={next}>{index + 1 === steps.length ? "Done" : "Next"}</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.tour {
		position: fixed;
		inset: 0;
		z-index: 85;
	}
	.scrim {
		position: absolute;
		inset: 0;
		border: 0;
		background: transparent;
		cursor: default;
	}
	/* The cutout: a transparent box whose huge shadow dims everything else. */
	.spot {
		position: absolute;
		border-radius: 12px;
		pointer-events: none;
		box-shadow:
			0 0 0 9999px color-mix(in oklab, black 52%, transparent),
			0 0 0 1.5px var(--space-accent, var(--ca-brand-main));
		transition:
			left 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
			top 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
			width 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
			height 240ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.card {
		position: absolute;
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 14px 16px 12px;
		border-radius: 14px;
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			var(--ca-shadow-lg, 0 18px 48px rgb(0 0 0 / 0.28));
		animation: arrive 220ms cubic-bezier(0.2, 0.8, 0.2, 1);
		transition:
			left 240ms cubic-bezier(0.2, 0.8, 0.2, 1),
			top 240ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.step {
		font-size: 11px;
		font-weight: 600;
		color: var(--space-accent, var(--ca-brand-main));
		font-variant-numeric: tabular-nums;
	}
	h3 {
		margin: 0;
		font-size: 14.5px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	p {
		margin: 0;
		font-size: 12.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.hint {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11.5px;
	}
	.foot {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 8px;
	}
	.dots {
		display: flex;
		gap: 4px;
		margin-right: auto;
	}
	.dots span {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--ca-outline-default, var(--ca-outline-subtle));
		transition:
			background 180ms ease-out,
			transform 180ms ease-out;
	}
	.dots span.seen {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 50%, transparent);
	}
	.dots span.on {
		background: var(--space-accent, var(--ca-brand-main));
		transform: scale(1.4);
	}
	.skip {
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 4px 6px;
		border-radius: 6px;
	}
	.skip:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(4px) scale(0.98);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spot,
		.card {
			transition: none;
			animation: none;
		}
	}
</style>
