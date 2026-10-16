<script lang="ts">
	/** Settings sidebar: search box, section list with the update badge, version footer. */
	import {
		Bot,
		CircleHelp,
		Code,
		Download,
		FolderTree,
		GitBranch,
		Keyboard,
		Palette,
		Server,
		SlidersHorizontal,
		SquareTerminal,
	} from "@lucide/svelte";
	import SearchField from "#lib/components/SearchField.svelte";
	import { SECTIONS, type SettingsSection } from "#lib/settings/index.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";

	const ICONS: Record<SettingsSection, typeof Download> = {
		general: SlidersHorizontal,
		appearance: Palette,
		editor: Code,
		terminal: SquareTerminal,
		workspaces: FolderTree,
		agents: Bot,
		git: GitBranch,
		keyboard: Keyboard,
		ssh: Server,
		help: CircleHelp,
	};

	let { searching }: { searching: boolean } = $props();
</script>

<nav aria-label="Settings sections">
	<div class="navtitle">Settings</div>
	<div class="find">
		<SearchField clearable placeholder="Search settings" aria-label="Search settings" bind:value={u.query} />
	</div>
	{#each SECTIONS as n (n.id)}
		{@const Icon = ICONS[n.id]}
		<button
			class="nav"
			class:on={!searching && u.section === n.id}
			aria-current={!searching && u.section === n.id ? "page" : undefined}
			onclick={() => {
				u.query = "";
				u.section = n.id;
			}}
		>
			<Icon size={15} />
			{n.label}
			{#if n.id === "general" && u.available && !u.linked}<span
					class="badge-dot"
					aria-label="Update available"
				></span>{/if}
		</button>
	{/each}
	<div class="navfoot mono">
		Atlas {u.info?.app.version ?? ""}{#if u.dev}<span class="devchip">Dev</span>{/if}
	</div>
</nav>

<style>
	nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 20px 12px 14px;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
	}
	.navtitle {
		font-size: 15px;
		font-weight: 700;
		padding: 0 8px 12px;
	}
	.find {
		display: flex;
		margin: 0 0 10px;
	}
	.nav {
		display: flex;
		align-items: center;
		gap: 9px;
		height: 32px;
		padding: 0 10px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 13px;
		font-weight: 500;
		text-align: left;
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.nav:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.nav.on {
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 9%, transparent);
	}
	.nav.on :global(svg) {
		color: var(--ca-brand-main);
	}
	.badge-dot {
		margin-left: auto;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-brand-main);
	}
	.navfoot {
		margin-top: auto;
		padding: 0 8px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.devchip {
		margin-left: 6px;
		padding: 1px 6px;
		border-radius: 999px;
		font-family: inherit;
		font-size: 9.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 16%, transparent);
	}
</style>
