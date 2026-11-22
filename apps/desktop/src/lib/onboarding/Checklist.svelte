<script lang="ts">
	import { Kbd } from "@codeatlasdev/ui";
	import { Check, ChevronDown, X } from "@lucide/svelte";
	import { slide } from "svelte/transition";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import CornerCard from "#lib/shell/CornerCard.svelte";
	import { corner } from "#lib/shell/corner.svelte.ts";
	import { reducedMotion } from "#lib/shell/cornerMotion.ts";
	import { onboarding } from "./state.svelte.ts";
	import { CHECKLIST, nextItem, progress } from "./checklist.ts";
	import { m } from "#lib/i18n/m.ts";

	let p = $derived(progress(onboarding.state));
	let next = $derived(nextItem(onboarding.state));
	let visible = $derived(
		onboarding.loaded &&
			!onboarding.firstRun &&
			!onboarding.state.checklist.dismissed &&
			!onboarding.tourOpen,
	);
	// The coordinator folds it to this header while a toast is up and opens it again after (when the user had it open).
	let open = $derived(!(corner.plan.checklist?.collapsed ?? true));

	// Finished: leave a moment for the last check to register, then step aside.
	$effect(() => {
		if (visible && p.all) {
			const t = setTimeout(() => void onboarding.dismissChecklist(), 2400);
			return () => clearTimeout(t);
		}
	});
</script>

{#if visible}
	<!-- Esc folds it (the X dismisses it for good, and "Show getting started checklist" brings it back). -->
	<CornerCard
		id="checklist"
		kind="checklist"
		role="complementary"
		label={m.onboarding_checklist_getting_started()}
		ondismiss={() => open && corner.toggleChecklist()}
	>
		<div class="inner" class:collapsed={!open} data-testid="checklist">
			<header>
				<button class="head" onclick={() => corner.toggleChecklist()} aria-expanded={open}>
					<span class="ring" style:--pct="{(p.done / p.total) * 100}%"></span>
					<span class="title"
						>{p.all ? m.onboarding_checklist_all_set() : m.onboarding_checklist_getting_started()}</span
					>
					<span class="count">{p.done}/{p.total}</span>
					<ChevronDown size={13} class="chev" />
				</button>
				<button
					class="x"
					aria-label={m.onboarding_checklist_dismiss_checklist()}
					onclick={() => void onboarding.dismissChecklist()}
				>
					<X size={12} />
				</button>
			</header>
			{#if open}
				<ul transition:slide={{ duration: reducedMotion() ? 0 : 200 }}>
					{#each CHECKLIST as c (c.id)}
						{@const done = onboarding.state.checklist.done.includes(c.id)}
						<li>
							<button
								class="item"
								class:done
								class:next={next?.id === c.id}
								disabled={done}
								onclick={() => c.command && runCommand(c.command)}
							>
								<span class="box"
									>{#if done}<Check size={10} strokeWidth={3} />{/if}</span
								>
								<span class="label">{c.label}</span>
								{#if c.command && !done && shortcutFor(c.command)}<Kbd>{shortcutFor(c.command)}</Kbd>{/if}
							</button>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	</CornerCard>
{/if}

<style>
	.inner {
		padding: 4px;
	}
	header {
		display: flex;
		align-items: center;
	}
	.head {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 8px;
		border: 0;
		border-radius: 10px;
		background: none;
		font: inherit;
		color: var(--ca-text-primary);
		text-align: left;
	}
	.head:hover,
	.x:hover {
		background: var(--ca-action-hover);
	}
	.ring {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: conic-gradient(
			var(--space-accent, var(--ca-brand-main)) var(--pct),
			color-mix(in oklab, var(--ca-text-primary) 12%, transparent) 0
		);
		mask: radial-gradient(circle, transparent 4.5px, #000 5px);
		-webkit-mask: radial-gradient(circle, transparent 4.5px, #000 5px);
		transition: background 300ms ease-out;
	}
	.title {
		flex: 1;
		font-size: 12.5px;
		font-weight: 600;
	}
	.count {
		font-size: 11px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.head :global(.chev) {
		color: var(--ca-text-secondary);
		transition: transform 200ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.collapsed .head :global(.chev) {
		transform: rotate(180deg);
	}
	.x {
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		margin-right: 4px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 2px 0 4px;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 9px;
		width: 100%;
		padding: 6px 8px;
		border: 0;
		border-radius: 8px;
		background: none;
		font: inherit;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		text-align: left;
		transition: background 120ms ease-out;
	}
	.item:not(:disabled):hover {
		background: var(--ca-action-hover);
	}
	.item.done {
		color: var(--ca-text-disabled);
	}
	.item.done .label {
		text-decoration: line-through;
		text-decoration-color: color-mix(in oklab, currentColor 50%, transparent);
	}
	.label {
		flex: 1;
	}
	.box {
		display: grid;
		place-items: center;
		width: 15px;
		height: 15px;
		border-radius: 50%;
		box-shadow: inset 0 0 0 1.5px var(--ca-outline-default, var(--ca-text-disabled));
		color: var(--ca-brand-contrast);
		transition:
			background 220ms cubic-bezier(0.2, 0.8, 0.2, 1),
			box-shadow 220ms ease-out;
	}
	.item.next .box {
		box-shadow: inset 0 0 0 1.5px var(--space-accent, var(--ca-brand-main));
	}
	.item.done .box {
		background: var(--space-accent, var(--ca-brand-main));
		box-shadow: none;
	}
	@media (prefers-reduced-motion: reduce) {
		.head :global(.chev),
		.box,
		.ring {
			transition: none;
		}
	}
</style>
