<script lang="ts">
	/**
	 * First run: a short deck, every scene drawn and animated in the page (SVG + CSS, no video), then the project steps
	 * (Phases) as its last slide. The sky behind it is two soft gradients that cross-fade to each slide's colour.
	 * → / Enter next, ← back, Esc skips (FirstRun owns Esc/Enter once the project steps are showing).
	 */
	import type { Component } from "svelte";
	import { Button } from "@codeatlasdev/ui";
	import { ArrowLeft, ArrowRight } from "@lucide/svelte";
	import { ACTIONS } from "../flow.ts";
	import Phases from "../Phases.svelte";
	import { welcome as w } from "../welcome.svelte.ts";
	import Mark from "./Mark.svelte";
	import SceneAgents from "./SceneAgents.svelte";
	import SceneScan from "./SceneScan.svelte";
	import SceneStack from "./SceneStack.svelte";
	import SceneWorktrees from "./SceneWorktrees.svelte";

	let {
		onfinish,
		onskip,
		atProjects = $bindable(false),
	}: {
		onfinish: () => void;
		onskip: () => void;
		/** The last slide (the project steps) is showing. */
		atProjects?: boolean;
	} = $props();

	interface Slide {
		title: string;
		body: string;
		/** Two sky colours (top-left, bottom-right) behind the slide. */
		sky: [string, string];
		scene?: Component;
	}
	const SLIDES: Slide[] = [
		{
			title: "Welcome to Atlas",
			body: "One place for everything you run: your stack, your terminals and your coding agents.",
			sky: ["#c9d2ff", "#f3c9e8"],
		},
		{
			title: "One file runs your whole stack.",
			body: "Atlas reads atlas.yaml, starts every service in order and shows you when each one is up.",
			sky: ["#bfe6d8", "#cfe3ff"],
			scene: SceneStack,
		},
		{
			title: "Agents that work side by side.",
			body: "Run Claude Code, Codex or omp in parallel, each in its own terminal, each on its own branch.",
			sky: ["#ffe0b3", "#ffd0c2"],
			scene: SceneAgents,
		},
		{
			title: "A worktree for every idea.",
			body: "Branch, try, review the diff and merge back, without stashing or juggling folders.",
			sky: ["#c6dcff", "#d9ccff"],
			scene: SceneWorktrees,
		},
	];
	const LAST = SLIDES.length; // the project steps

	let i = $state(0);
	let dir = $state(1);
	const go = (n: number) => {
		if (n < 0 || n > LAST) return;
		dir = n > i ? 1 : -1;
		i = n;
	};
	$effect(() => {
		atProjects = i === LAST;
	});
	const sky = $derived(i < LAST ? SLIDES[i]!.sky : (["#d6dcff", "#cfeee6"] as [string, string]));

	function onkeydown(e: KeyboardEvent) {
		if (i === LAST || e.metaKey || e.ctrlKey || e.altKey || e.defaultPrevented) return;
		if ((e.target as HTMLElement | null)?.closest("button, input, a")) return;
		if (e.key === "ArrowRight" || e.key === "Enter") go(i + 1);
		else if (e.key === "ArrowLeft") go(i - 1);
		else return;
		e.preventDefault();
	}
</script>

<svelte:window {onkeydown} />

<div class="deck" style:--a={sky[0]} style:--b={sky[1]} data-slide={i}>
	<div class="sky a" aria-hidden="true"></div>
	<div class="sky b" aria-hidden="true"></div>

	{#key i}
		<section class="slide" style:--dir={dir} aria-live="polite">
			{#if i < LAST}
				{@const s = SLIDES[i]!}
				<div class="visual" class:mark={!s.scene}>
					{#if s.scene}<s.scene />{:else}<Mark size={112} />{/if}
				</div>
				<h1>{s.title}</h1>
				<p class="lead">{s.body}</p>
				<div class="actions">
					<Button size="lg" onclick={() => go(i + 1)}>
						{i === 0 ? "Get started" : "Continue"}
						{#snippet trailing()}<ArrowRight size={15} />{/snippet}
					</Button>
				</div>
			{:else}
				<div class="phases">
					{#if w.phase === "welcome"}<SceneScan />{/if}
					<Phases {onfinish} />
				</div>
			{/if}
		</section>
	{/key}

	<nav class="dots" aria-label="Steps">
		{#each [...SLIDES, null] as _, k (k)}
			<button
				class:on={k === i}
				aria-label="Step {k + 1} of {LAST + 1}"
				aria-current={k === i ? "step" : undefined}
				onclick={() => go(k)}
			></button>
		{/each}
	</nav>
	{#if i > 0 && w.phase === "welcome"}
		<button class="back" onclick={() => go(i - 1)} aria-label="Back"><ArrowLeft size={15} /></button>
	{/if}
	{#if w.phase !== "up"}
		<button class="skip" data-testid={ACTIONS.skip} onclick={onskip}>Skip for now</button>
	{/if}
</div>

<style>
	.deck {
		position: absolute;
		inset: 0;
		overflow: hidden;
		isolation: isolate;
		display: grid;
		place-items: center;
		background: #f6f7fb;
		color: var(--ca-text-primary);
		/* Light stage whatever the app theme, like the reference: the scene cards keep the kit's light tokens. */
		--ca-text-primary: #14161c;
		--ca-text-secondary: #5b6170;
		--ca-text-disabled: #b4b9c4;
		--ca-surface-raised: #ffffff;
		--ca-surface-sunken: #f1f3f7;
		--ca-outline-subtle: #0000000f;
		--ca-outline-default: #00000017;
		--ca-outline-strong: #0000002b;
	}
	/* Two layers, each a soft blob pair; only their opacity and transform change between slides (compositor only). */
	.sky {
		position: absolute;
		inset: -20%;
		z-index: -1;
		pointer-events: none;
		transition:
			background 900ms ease,
			transform 1400ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.sky.a {
		background: radial-gradient(45% 50% at 22% 20%, var(--a), transparent 70%);
	}
	.sky.b {
		background: radial-gradient(45% 55% at 82% 78%, var(--b), transparent 70%);
		animation: drift 18s ease-in-out infinite alternate;
	}
	@keyframes drift {
		to {
			transform: translate(-4%, -3%) scale(1.05);
		}
	}
	.slide {
		width: min(640px, calc(100% - 64px));
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 12px;
		animation: enter 560ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	@keyframes enter {
		from {
			opacity: 0;
			transform: translateX(calc(var(--dir) * 28px));
		}
	}
	.visual {
		width: 100%;
		aspect-ratio: 16 / 7.4;
		margin-bottom: 18px;
		animation: rise 700ms 80ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	.visual.mark {
		aspect-ratio: auto;
		height: 150px;
		display: grid;
		place-items: center;
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(14px) scale(0.98);
		}
	}
	.slide > h1,
	.slide > .lead,
	.slide > .actions {
		animation: rise 560ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
	}
	.slide > h1 {
		animation-delay: 140ms;
	}
	.slide > .lead {
		animation-delay: 210ms;
	}
	.slide > .actions {
		animation-delay: 280ms;
	}
	h1 {
		margin: 0;
		font-size: 28px;
		line-height: 1.1;
		font-weight: 720;
		letter-spacing: -0.03em;
		text-wrap: balance;
	}
	.lead {
		margin: 0;
		max-width: 44ch;
		font-size: 14px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
		text-wrap: pretty;
	}
	.actions {
		margin-top: 14px;
	}
	.phases {
		width: min(460px, 100%);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
	}
	.dots {
		position: absolute;
		bottom: 26px;
		left: 50%;
		translate: -50% 0;
		display: flex;
		gap: 8px;
	}
	.dots button {
		width: 6px;
		height: 6px;
		padding: 0;
		border: 0;
		border-radius: 3px;
		background: #14161c33;
		transition:
			width 300ms cubic-bezier(0.2, 0.8, 0.2, 1),
			background-color 300ms;
	}
	.dots button.on {
		width: 18px;
		background: #14161c;
	}
	.back,
	.skip {
		position: absolute;
		bottom: 18px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 6px 9px;
		border-radius: 8px;
	}
	.back {
		left: 20px;
		display: grid;
		place-items: center;
	}
	.skip {
		right: 20px;
	}
	.back:hover,
	.skip:hover {
		background: #0000000d;
		color: var(--ca-text-primary);
	}
	@media (prefers-reduced-motion: reduce) {
		.slide,
		.slide > *,
		.visual,
		.sky.b {
			animation: none;
		}
	}
</style>
