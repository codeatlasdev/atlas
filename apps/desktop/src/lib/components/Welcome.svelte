<script lang="ts">
	import { Kbd } from "@codeatlasdev/ui";
	import { Bot, FileText, SquareTerminal } from "@lucide/svelte";
	import type { IconComponent } from "@codeatlasdev/ui";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	let { worktreeId }: { worktreeId: string } = $props();

	interface Action {
		label: string;
		hint: string;
		icon: IconComponent;
		command: string;
		run: () => void;
	}

	let actions = $derived<Action[]>([
		{
			label: "New terminal",
			hint: "A shell in this worktree",
			icon: SquareTerminal,
			command: "workspace::NewTerminal",
			run: () => void ws.newTerminal(worktreeId),
		},
		{
			label: "Start an agent",
			hint: `Run ${agents.list.find((a) => a.id === agents.defaultAgent)?.label ?? "your default agent"} here`,
			icon: Bot,
			command: "atlas::NewAgentSession",
			run: () => void agents.launch(worktreeId),
		},
		{
			label: "Open a file",
			hint: "Fuzzy-find in this worktree",
			icon: FileText,
			command: "file_finder::Toggle",
			run: () => runCommand("file_finder::Toggle"),
		},
	]);
</script>

<div class="welcome">
	<div class="inner">
		<h2>Nothing open here</h2>
		<p>Pick up where you left off, or start something new.</p>
		<div class="actions">
			{#each actions as a (a.command)}
				<button class="action" onclick={a.run}>
					<span class="ico"><a.icon size={16} /></span>
					<span class="text">
						<b>{a.label}</b>
						<small>{a.hint}</small>
					</span>
					{#if shortcutFor(a.command)}<Kbd>{shortcutFor(a.command)}</Kbd>{/if}
				</button>
			{/each}
		</div>
		<p class="foot">
			<Kbd>{shortcutFor("command_palette::Toggle") ?? "⌘⇧P"}</Kbd> for every command
			<span aria-hidden="true">·</span>
			<button class="link" onclick={() => runCommand("atlas::Help")}>Shortcuts</button>
			<span aria-hidden="true">·</span>
			<button class="link" onclick={() => runCommand("atlas::TakeTour")}>Take the tour</button>
		</p>
	</div>
</div>

<style>
	.welcome {
		flex: 1;
		display: grid;
		place-items: center;
		padding: 24px;
		min-width: 0;
	}
	.inner {
		width: min(360px, 100%);
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	p {
		margin: 0 0 10px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.actions {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.action {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 9px;
		background: var(--ca-surface-raised);
		color: var(--ca-text-primary);
		font: inherit;
		text-align: left;
		transition:
			background 120ms ease-out,
			border-color 120ms ease-out,
			transform 120ms ease-out;
	}
	.action:hover {
		background: var(--ca-action-hover);
		border-color: var(--ca-outline-default, var(--ca-outline-subtle));
	}
	.action:active {
		transform: scale(0.985);
	}
	.ico {
		display: grid;
		place-items: center;
		flex-shrink: 0;
		width: 30px;
		height: 30px;
		border-radius: 8px;
		color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-brand-main) 12%, transparent);
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		line-height: 1.3;
	}
	.text b {
		font-size: 13px;
		font-weight: 600;
	}
	.text small {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.foot {
		margin: 14px 0 0;
		font-size: 11.5px;
		color: var(--ca-text-disabled);
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--ca-text-secondary);
	}
	.link:hover {
		color: var(--ca-text-primary);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
</style>
