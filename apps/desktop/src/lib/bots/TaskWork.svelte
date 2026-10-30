<!--
	What a crewmate did for a task, as sections of the conversation: one per title the agent set itself
	("CART: Testar store api e carrinho"). The section in progress is open, its steps ticking off like the onboarding
	story; a finished one folds to its title with a check and opens on click. It stays in the chat after the task
	ends, before the reply, instead of a small "N steps" toggle under it.
-->
<script lang="ts">
	import { Check, ChevronDown, ChevronRight } from "@lucide/svelte";
	import type { BotStep } from "#lib/bots.ts";
	import { stepGroups, stepPhrase } from "./stepPhrase.ts";

	let { steps, running, title }: { steps: BotStep[]; running: boolean; title?: string } = $props();

	/** Open sections by index; a finished one is folded until clicked, the one in progress is always open. */
	let open = $state<Record<number, boolean>>({});
	const groups = $derived(stepGroups(steps, running, title));
	/** The section in progress shows its last few steps; the rest are counted. */
	const LIVE_ROWS = 6;
</script>

{#if groups.length > 0}
	<div class="work">
		{#each groups as g, gi (gi)}
			{@const expanded = !g.done || !!open[gi]}
			{@const rows = g.done ? g.steps : g.steps.slice(-LIVE_ROWS)}
			<section class="group" class:done={g.done}>
				{#if g.done}
					<button class="head" aria-expanded={expanded} onclick={() => (open = { ...open, [gi]: !open[gi] })}>
						<span class="tick" aria-hidden="true"><Check size={11} /></span>
						<span class="title">{g.title ?? "Work"}</span>
						<span class="count">{g.steps.length} {g.steps.length === 1 ? "step" : "steps"}</span>
						{#if expanded}<ChevronDown size={13} aria-hidden="true" />{:else}<ChevronRight
								size={13}
								aria-hidden="true"
							/>{/if}
					</button>
				{:else if g.title}
					<div class="head live"><span class="title">{g.title}</span></div>
				{/if}
				{#if expanded}
					{#if g.steps.length > rows.length}
						<div class="more">+{g.steps.length - rows.length} earlier</div>
					{/if}
					<ul class="steps">
						{#each rows as step, si (si + g.steps.length - rows.length)}
							{@const current = !g.done && si === rows.length - 1}
							<li class:current>
								<span class="tick" class:current aria-hidden="true">
									{#if !current}<Check size={11} />{/if}
								</span>
								<span class="text">{stepPhrase(step.text)}</span>
							</li>
						{/each}
					</ul>
				{/if}
			</section>
		{/each}
	</div>
{/if}

<style>
	.work {
		width: min(420px, 100%);
		box-sizing: border-box;
		margin: 8px 0 6px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.group {
		padding: 10px 14px;
		border-radius: 14px;
		background: var(--ca-surface-sunken);
		border: 0.5px solid var(--ca-outline-subtle);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.group.done {
		padding: 8px 12px;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		color: var(--ca-text-secondary);
	}
	button.head {
		all: unset;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		cursor: pointer;
		color: var(--ca-text-secondary);
	}
	button.head:focus-visible {
		outline: 2px solid var(--ca-brand-main);
		outline-offset: 2px;
		border-radius: 6px;
	}
	.title {
		flex: 1;
		min-width: 0;
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.head.live .title {
		font-size: 14px;
		white-space: normal;
		overflow-wrap: anywhere;
	}
	.count {
		flex: none;
		font-size: 12px;
	}
	.more {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.steps li {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--ca-text-secondary);
		min-width: 0;
	}
	.steps li.current {
		color: var(--ca-text-primary);
	}
	.text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tick {
		flex: none;
		display: inline-grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--ca-semantic-success) 16%, transparent);
	}
	.tick.current {
		background: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 22%, transparent);
		animation: pulse 1.4s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			transform: scale(0.75);
			opacity: 0.6;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.tick.current {
			animation: none;
		}
	}
</style>
