<script lang="ts">
	import { Button, Kbd } from "@codeatlasdev/ui";
	import { FolderPlus, Layers, Search, SquareTerminal } from "@lucide/svelte";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { onboarding } from "./state.svelte.ts";

	const BEATS = [
		{
			icon: Layers,
			title: "Projects become spaces",
			text: "Each project gets its own panel, color and memory of what was open.",
		},
		{
			icon: SquareTerminal,
			title: "Terminals and agents per worktree",
			text: "Branch off, and everything you run stays with that branch.",
		},
		{
			icon: Search,
			title: "One key to go anywhere",
			text: "Jump to a worktree, file or command by typing a few letters.",
			key: "atlas::Jump",
		},
	];

	let adding = $derived(!!store.busy["add"]);

	// A project showing up (any path: picker, discovery, CLI) ends the welcome and starts the tour.
	let started = false;
	$effect(() => {
		if (!store.projects.length || started) return;
		started = true;
		void onboarding.finishWelcome().then(() => setTimeout(() => onboarding.startTour(), 600));
	});
</script>

<div class="first" role="dialog" aria-label="Welcome to Atlas">
	<div class="drag" data-tauri-drag-region></div>
	<div class="inner">
		<div class="mark" aria-hidden="true"></div>
		<h1>Welcome to Atlas</h1>
		<p class="lead">Your local dev environment, one calm place.</p>
		<ul>
			{#each BEATS as b, i (b.title)}
				<li style:animation-delay="{120 + i * 90}ms">
					<span class="ico"><b.icon size={16} /></span>
					<span class="text">
						<b>{b.title}</b>
						<small>{b.text}</small>
					</span>
					{#if b.key}<Kbd>{shortcutFor(b.key) ?? "⌘K"}</Kbd>{/if}
				</li>
			{/each}
		</ul>
		<div class="cta">
			<Button size="lg" disabled={adding} onclick={() => runCommand("atlas::AddProject")}>
				{#snippet leading()}<FolderPlus size={15} />{/snippet}
				Add your first project
			</Button>
			<button class="link" onclick={() => runCommand("atlas::DiscoverProjects")}>
				Or find projects on this machine
			</button>
		</div>
		<p class="hint">A project is any folder with an <code>atlas.yaml</code>.</p>
	</div>
	<button class="skip" onclick={() => void onboarding.finishWelcome()}>Skip for now</button>
</div>

<style>
	.first {
		position: fixed;
		inset: 0;
		z-index: 80;
		display: grid;
		place-items: center;
		background: color-mix(in oklab, var(--ca-surface-base) 82%, transparent);
		backdrop-filter: blur(28px) saturate(1.4);
		-webkit-backdrop-filter: blur(28px) saturate(1.4);
		animation: fade 320ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.drag {
		position: absolute;
		inset: 0 0 auto 0;
		height: 40px;
	}
	.inner {
		width: min(420px, calc(100% - 48px));
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 6px;
	}
	.mark {
		width: 44px;
		height: 44px;
		margin-bottom: 10px;
		border-radius: 13px;
		background: linear-gradient(
			140deg,
			var(--ca-brand-main),
			color-mix(in oklab, var(--ca-brand-main) 40%, #c26cff)
		);
		box-shadow: 0 8px 24px color-mix(in oklab, var(--ca-brand-main) 40%, transparent);
	}
	h1 {
		margin: 0;
		font-size: 26px;
		font-weight: 700;
		letter-spacing: -0.025em;
	}
	.lead {
		margin: 0 0 16px;
		font-size: 14px;
		color: var(--ca-text-secondary);
	}
	ul {
		list-style: none;
		margin: 0 0 20px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 9px 12px;
		border-radius: 11px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		animation: rise 360ms cubic-bezier(0.2, 0.8, 0.2, 1) backwards;
	}
	.ico {
		display: grid;
		place-items: center;
		flex-shrink: 0;
		width: 32px;
		height: 32px;
		border-radius: 9px;
		color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-brand-main) 12%, transparent);
	}
	.text {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 0;
		line-height: 1.35;
	}
	.text b {
		font-size: 13px;
		font-weight: 600;
	}
	.text small {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.cta {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		gap: 8px;
	}
	.link {
		border: 0;
		background: none;
		font: inherit;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		padding: 6px;
		border-radius: 8px;
	}
	.link:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.hint {
		margin: 6px 0 0;
		text-align: center;
		font-size: 11.5px;
		color: var(--ca-text-disabled);
	}
	code {
		font-family: var(--mono);
	}
	.skip {
		position: absolute;
		right: 20px;
		bottom: 18px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-disabled);
		padding: 5px 8px;
		border-radius: 7px;
	}
	.skip:hover {
		color: var(--ca-text-secondary);
		background: var(--ca-action-hover);
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.first,
		li {
			animation: none;
		}
	}
</style>
