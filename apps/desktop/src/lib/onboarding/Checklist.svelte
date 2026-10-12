<script lang="ts">
	import { Kbd } from "@codeatlasdev/ui";
	import { Check, ChevronDown, X } from "@lucide/svelte";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import { onboarding } from "./state.svelte.ts";
	import { CHECKLIST, nextItem, progress } from "./checklist.ts";

	const KEY = "atlas.checklistOpen";
	let open = $state(localStorage.getItem(KEY) !== "0");
	let p = $derived(progress(onboarding.state));
	let next = $derived(nextItem(onboarding.state));
	let visible = $derived(
		onboarding.loaded &&
			!onboarding.firstRun &&
			!onboarding.state.checklist.dismissed &&
			!onboarding.tourOpen,
	);

	function toggle() {
		open = !open;
		localStorage.setItem(KEY, open ? "1" : "0");
	}

	// Finished: leave a moment for the last check to register, then step aside.
	$effect(() => {
		if (visible && p.all) {
			const t = setTimeout(() => void onboarding.dismissChecklist(), 2400);
			return () => clearTimeout(t);
		}
	});
</script>

{#if visible}
	<aside class="card" class:collapsed={!open} aria-label="Getting started">
		<header>
			<button class="head" onclick={toggle} aria-expanded={open}>
				<span class="ring" style:--pct="{(p.done / p.total) * 100}%"></span>
				<span class="title">{p.all ? "You're all set" : "Getting started"}</span>
				<span class="count">{p.done}/{p.total}</span>
				<ChevronDown size={13} class="chev" />
			</button>
			<button class="x" aria-label="Dismiss checklist" onclick={() => void onboarding.dismissChecklist()}>
				<X size={12} />
			</button>
		</header>
		{#if open}
			<ul>
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
	</aside>
{/if}

<style>
	.card {
		position: fixed;
		right: 20px;
		bottom: 20px;
		z-index: 60;
		width: 250px;
		padding: 4px;
		border-radius: 14px;
		background: color-mix(in oklab, var(--ca-surface-raised) 88%, transparent);
		backdrop-filter: blur(20px) saturate(1.4);
		-webkit-backdrop-filter: blur(20px) saturate(1.4);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			var(--ca-shadow-md, 0 10px 30px rgb(0 0 0 / 0.2));
		animation: arrive 260ms cubic-bezier(0.2, 0.8, 0.2, 1);
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
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(10px) scale(0.97);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.card {
			animation: none;
		}
		.head :global(.chev),
		.box,
		.ring {
			transition: none;
		}
	}
</style>
