<script lang="ts">
	/** Sidebar footer: update pill, terminal-host chip, space dots, add/find project, settings. */
	import { IconButton, Kbd, Spinner } from "@codeatlasdev/ui";
	import { Plus, Search, Settings, Sparkles, TerminalSquare } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { chipFor } from "#lib/hostStatus.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import SpaceDots from "#lib/shell/SpaceDots.svelte";
	import { hostState, openHostPrompt } from "#lib/stores/hostStatus.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { addProject } from "./projects.ts";

	let { onDiscover }: { onDiscover: () => void } = $props();
	let chip = $derived(chipFor(hostState));
	/** Something to decide opens the restart prompt; the calm "version differs" chip opens Settings. */
	const openChip = () => (chip?.tone === "warn" ? openHostPrompt() : updates.show("updates"));
</script>

<footer>
	{#if updates.pill}
		<button class="pill" onclick={updates.showWhatsNew} aria-label="Update to {updates.release?.version}">
			<Sparkles size={13} />
			<span>Update to {updates.release?.version}</span>
		</button>
	{/if}
	{#if chip}
		<Tip label={chip.detail || chip.label} side="top">
			{#snippet child(props)}
				<button
					{...props}
					class="host {chip.tone}"
					disabled={chip.tone === "busy"}
					onclick={openChip}
					aria-label={chip.label}
				>
					{#if chip.tone === "busy"}<Spinner size={12} />{:else}<TerminalSquare size={13} />{/if}
					<span>{chip.label}</span>
				</button>
			{/snippet}
		</Tip>
	{/if}
	<SpaceDots />
	<button class="add" onclick={addProject}><Plus size={14} /> Add project…</button>
	<div class="tools">
		<Tip label="Find projects" side="top">
			{#snippet child(props)}
				<IconButton {...props} variant="text" size="sm" aria-label="Find projects" onclick={onDiscover}>
					<Search size={14} />
				</IconButton>
			{/snippet}
		</Tip>
		<Tip label="Settings" command="zed::OpenSettings" side="top">
			{#snippet child(props)}
				<IconButton {...props} variant="text" size="sm" aria-label="Settings" onclick={() => updates.show()}>
					<Settings size={14} />
				</IconButton>
			{/snippet}
		</Tip>
		<Kbd>{shortcutFor("atlas::Jump") ?? "⌘K"}</Kbd>
	</div>
</footer>

<style>
	footer {
		padding: 10px;
		display: flex;
		flex-direction: column;
		gap: 6px;
		border-top: 0.5px solid var(--ca-vibrancy-separator);
	}
	.pill {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 7px;
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: 999px;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		color: var(--ca-brand-main);
		animation: rise 260ms var(--ca-ease-spring, ease-out);
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-raised));
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-brand-main) 45%, transparent),
			0 0 14px -2px color-mix(in oklab, var(--ca-brand-main) 45%, transparent);
		transition:
			transform 120ms ease-out,
			box-shadow 160ms ease-out;
	}
	.pill:hover {
		box-shadow:
			0 0 0 0.5px var(--ca-brand-main),
			0 0 18px -1px color-mix(in oklab, var(--ca-brand-main) 60%, transparent);
	}
	.pill:active {
		transform: scale(0.98);
	}
	@media (prefers-reduced-motion: no-preference) {
		.pill {
			animation: rise 260ms var(--ca-ease-spring, ease-out);
		}
		.pill::after {
			animation: glow 3.2s ease-in-out 600ms infinite;
		}
	}
	/* The glow is a static shadow on a pseudo-element whose opacity breathes: the blurred shadow is painted once. */
	.pill::after {
		content: "";
		position: absolute;
		inset: 0;
		border-radius: inherit;
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-brand-main) 55%, transparent),
			0 0 20px 0 color-mix(in oklab, var(--ca-brand-main) 55%, transparent);
		opacity: 0;
		pointer-events: none;
	}
	@keyframes glow {
		50% {
			opacity: 1;
		}
	}
	@keyframes rise {
		from {
			transform: translateY(6px);
			opacity: 0;
		}
	}
	.host {
		display: flex;
		align-items: center;
		gap: 7px;
		height: 26px;
		padding: 0 10px;
		border: 0;
		border-radius: 7px;
		font: inherit;
		font-size: 11.5px;
		font-weight: 500;
		text-align: left;
		cursor: pointer;
		color: var(--ca-text-secondary);
		background: var(--ca-vibrancy-fill);
	}
	.host span {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.host.warn {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 12%, transparent);
	}
	.host.warn:hover {
		background: color-mix(in oklab, var(--ca-semantic-warning) 20%, transparent);
	}
	.host.info:hover {
		color: var(--ca-text-primary);
	}
	.host.busy {
		cursor: default;
	}
	.add {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 30px;
		padding: 0 8px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}
	.add:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.tools {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
	}
	.tools :global(.ca-kbd) {
		margin-left: auto;
	}
</style>
