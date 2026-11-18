<script lang="ts">
	import { Kbd } from "@codeatlasdev/ui";
	import { Bot, FileText, SquareTerminal } from "@lucide/svelte";
	import type { IconComponent } from "@codeatlasdev/ui";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { runCommand, shortcutFor } from "#lib/keymap.svelte.ts";
	import { kbd } from "#lib/keys.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { aroundSlot } from "#lib/i18n/slot.ts";
	import { m } from "#lib/i18n/m.ts";

	let { worktreeId }: { worktreeId: string } = $props();
	/** A plain folder: its files are the point, so the start-something-new cards stay out of the way. */
	let plain = $derived(!!ws.worktree(worktreeId)?.plain);

	interface Action {
		label: string;
		hint: string;
		icon: IconComponent;
		command: string;
		run: () => void;
	}

	let defaultAgent = $derived(agents.list.find((a) => a.id === agents.defaultAgent)?.label);
	let actions = $derived<Action[]>([
		{
			label: m.onboarding_welcome_new_terminal(),
			hint: m.onboarding_welcome_new_terminal_hint(),
			icon: SquareTerminal,
			command: "workspace::NewTerminal",
			run: () => void ws.newTerminal(worktreeId),
		},
		{
			label: m.onboarding_welcome_start_agent(),
			hint: defaultAgent
				? m.onboarding_welcome_start_agent_hint({ name: defaultAgent })
				: m.onboarding_welcome_start_agent_hint_default(),
			icon: Bot,
			command: "atlas::NewAgentSession",
			run: () => void agents.launch(worktreeId),
		},
		{
			label: m.onboarding_welcome_open_file(),
			hint: m.onboarding_welcome_open_file_hint(),
			icon: FileText,
			command: "file_finder::Toggle",
			run: () => runCommand("file_finder::Toggle"),
		},
	]);
	// The shortcut sits inside its sentence as a key cap: the message places it, the template draws it.
	let findFile = $derived(aroundSlot((key) => m.onboarding_welcome_choose_or_search({ key })));
	let allCommands = $derived(aroundSlot((key) => m.onboarding_welcome_all_commands({ key })));
</script>

<div class="welcome">
	<div class="inner">
		{#if plain}
			<h2>{m.onboarding_welcome_pick_a_file()}</h2>
			<p>
				{findFile[0]}<Kbd>{shortcutFor("file_finder::Toggle") ?? kbd("cmd-p")}</Kbd>{findFile[1]}
			</p>
		{:else}
			<h2>{m.onboarding_welcome_nothing_open()}</h2>
			<p>{m.onboarding_welcome_start_something()}</p>
		{/if}
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
			{allCommands[0]}<Kbd>{shortcutFor("command_palette::Toggle") ?? kbd("cmd-shift-p")}</Kbd
			>{allCommands[1]}
			<span aria-hidden="true">·</span>
			<button class="link" onclick={() => runCommand("atlas::Help")}
				>{m.onboarding_welcome_shortcuts()}</button
			>
			<span aria-hidden="true">·</span>
			<button class="link" onclick={() => runCommand("atlas::TakeTour")}
				>{m.onboarding_welcome_take_the_tour()}</button
			>
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
		color: var(--ca-text-secondary);
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
