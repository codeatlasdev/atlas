<script lang="ts">
	/** One theme in the picker: live miniature, name, badges and the overflow menu (export / remove). */
	import { DropdownMenu, DropdownMenuItem, DropdownMenuSeparator } from "@codeatlasdev/ui";
	import { Check, Ellipsis, Moon, Sun } from "@lucide/svelte";
	import { DEFAULT_SELECTION, type Theme } from "#lib/theme.ts";
	import ThemePreview from "./ThemePreview.svelte";
	import { previewModel } from "./preview.ts";
	import { appearanceLabel } from "./labels.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		theme,
		selected,
		tabindex,
		onpick,
		oncopy,
		onsave,
		onremove,
		cardEl = $bindable(),
	}: {
		theme: Theme;
		selected: boolean;
		tabindex: 0 | -1;
		onpick: () => void;
		oncopy: (format: "json" | "ghostty") => void;
		onsave: (format: "json" | "ghostty") => void;
		onremove: () => void;
		cardEl?: HTMLElement;
	} = $props();

	let model = $derived(previewModel(theme));
	let isDefault = $derived(
		theme.source === "bundled" &&
			(theme.name === DEFAULT_SELECTION.dark || theme.name === DEFAULT_SELECTION.light),
	);
	let Icon = $derived(theme.appearance === "dark" ? Moon : Sun);
	/** Dracula and Alucard credit their author on the card (data-driven by the family file). */
	let byline = $derived(theme.source === "bundled" && theme.family === "Dracula");
	/** The card's spoken name: the theme, its appearance and each badge it carries, in one list. */
	let spoken = $derived(
		[
			theme.name,
			appearanceLabel(theme.appearance),
			byline && m.settings_theme_card_byline(),
			isDefault && m.settings_theme_card_default(),
			theme.source === "user" && m.settings_theme_card_yours(),
		]
			.filter(Boolean)
			.join(", "),
	);
</script>

<div class="card" class:selected>
	<!-- svelte-ignore a11y_no_noninteractive_element_to_interactive_role -->
	<div
		bind:this={cardEl}
		class="hit"
		role="radio"
		aria-checked={selected}
		aria-label={spoken}
		{tabindex}
		data-theme-card={theme.name}
		onclick={onpick}
		onkeydown={(e) => {
			if (e.key === "Enter" || e.key === " ") {
				e.preventDefault();
				onpick();
			}
		}}
	>
		<ThemePreview {model} />
		<div class="meta">
			<Icon size={12} aria-hidden="true" />
			<span class="name">{theme.name}</span>
			{#if isDefault}<span class="badge">{m.settings_theme_card_default()}</span>{/if}
			{#if theme.source === "user"}<span class="badge yours">{m.settings_theme_card_yours()}</span>{/if}
			{#if selected}<Check size={13} class="tick" aria-hidden="true" />{/if}
		</div>
		{#if byline}<span class="by">{m.settings_theme_card_byline()}</span>{/if}
	</div>
	<div class="more">
		<DropdownMenu align="end">
			{#snippet triggerChild(props)}
				<button {...props} class="menu-btn" aria-label={m.settings_theme_card_options({ name: theme.name })}>
					<Ellipsis size={14} />
				</button>
			{/snippet}
			<DropdownMenuItem onclick={() => oncopy("json")}>
				{m.settings_theme_tab_copy_as_json()}
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => oncopy("ghostty")}>
				{m.settings_theme_tab_copy_as_ghostty_config()}
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => onsave("json")}>
				{m.settings_theme_card_save_as_json()}
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => onsave("ghostty")}>
				{m.settings_theme_card_save_as_ghostty_config()}
			</DropdownMenuItem>
			{#if theme.source === "user"}
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onclick={onremove}>
					{m.common_action_delete()}
				</DropdownMenuItem>
			{/if}
		</DropdownMenu>
	</div>
</div>

<style>
	.card {
		position: relative;
		border-radius: 12px;
		min-width: 0;
	}
	.hit {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 6px 6px 8px;
		border-radius: 12px;
		background: var(--ca-surface-base);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		cursor: pointer;
		outline: none;
	}
	.hit:hover {
		background: var(--ca-action-hover);
	}
	.selected .hit {
		box-shadow:
			0 0 0 2px var(--atlas-focus-ring, var(--ca-brand-main)),
			0 0 0 0.5px var(--ca-outline-subtle);
	}
	.hit:focus-visible {
		box-shadow:
			0 0 0 2px var(--ca-surface-raised),
			0 0 0 4px var(--atlas-focus-ring, var(--ca-brand-main));
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 0 4px;
		min-width: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-weight: 550;
		color: var(--ca-text-primary);
	}
	.by {
		padding: 0 4px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}
	.badge {
		flex-shrink: 0;
		padding: 0 5px;
		border-radius: 999px;
		font-size: 10px;
		line-height: 15px;
		color: var(--ca-text-secondary);
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.badge.yours {
		color: var(--ca-brand-main);
	}
	.meta :global(.tick) {
		flex-shrink: 0;
		color: var(--atlas-focus-ring, var(--ca-brand-main));
	}
	.more {
		position: absolute;
		top: 10px;
		right: 10px;
		opacity: 0;
		transition: opacity 0.12s;
	}
	.card:hover .more,
	.card:focus-within .more,
	.more:has([data-state="open"]) {
		opacity: 1;
	}
	.menu-btn {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.menu-btn:focus-visible {
		outline: 2px solid var(--atlas-focus-ring, var(--ca-brand-main));
		outline-offset: 1px;
	}
</style>
