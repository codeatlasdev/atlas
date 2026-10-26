<script lang="ts">
	/** One component of the install (CLI, app, terminal host): icon, name, version, muted detail line. */
	import type { Component, Snippet } from "svelte";

	let {
		icon: Icon,
		name,
		version,
		sub,
		mono = false,
		tone = "neutral",
		title,
		badges,
		action,
	}: {
		icon: Component<{ size?: number }>;
		name: string;
		/** The installed version, mono. */
		version: Snippet | string;
		/** Muted second line: a path, a state. Truncates to one line; the full text is its tooltip. */
		sub: string;
		/** Monospace sub (paths). */
		mono?: boolean;
		/** `warn` tints the icon (older than the app, dev build). */
		tone?: "neutral" | "warn" | "info";
		/** Native tooltip with the untruncated detail. */
		title?: string;
		/** Small pills next to the name (Dev, linked, Postponed). */
		badges?: Snippet;
		/** Trailing control (a Restart button). */
		action?: Snippet;
	} = $props();
</script>

<div class="tile tone-{tone}" {title}>
	<div class="ico"><Icon size={15} /></div>
	<div class="what">
		<div class="top">
			<span class="name">{name}</span>
			{@render badges?.()}
		</div>
		<div class="ver mono">
			{#if typeof version === "string"}{version}{:else}{@render version()}{/if}
		</div>
		<div class="sub" class:mono>{sub}</div>
	</div>
	{#if action}<div class="act">{@render action()}</div>{/if}
</div>

<style>
	.tile {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		min-width: 0;
		padding: 11px 12px;
		border-radius: 10px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.ico {
		flex: none;
		width: 28px;
		height: 28px;
		border-radius: 8px;
		display: grid;
		place-items: center;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
	}
	.tone-warn .ico {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 14%, transparent);
	}
	.tone-info .ico {
		color: var(--ca-semantic-info);
		background: color-mix(in oklab, var(--ca-semantic-info) 14%, transparent);
	}
	.what {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.top {
		display: flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
	}
	.name {
		font-weight: 600;
		font-size: 12.5px;
		white-space: nowrap;
	}
	.ver {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		color: var(--ca-text-primary);
	}
	.sub {
		font-size: 11px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.act {
		flex: none;
		align-self: center;
	}
	.act :global(button) {
		white-space: nowrap;
	}
</style>
