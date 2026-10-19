<script lang="ts">
	/**
	 * The official logo of a coding agent, or the generic robot for an agent without one. See `lib/agent-logos/README.md`.
	 * The light/dark mark is the kit's `SchemeAsset` (CSS on `.dark`, so no theme store and no flash on first paint).
	 * In a kit `icon` slot (`DropdownMenuItem`, `TabsTrigger`) pass `bare`: the slot already draws the 14px box.
	 */
	import { Bot } from "@lucide/svelte";
	import { SchemeAsset } from "@codeatlasdev/ui";
	import { agentLogo } from "#lib/agentLogos.ts";

	let {
		agent,
		size = 14,
		bare = false,
	}: { agent: string | undefined; size?: number; bare?: boolean } = $props();

	const logo = $derived(agentLogo(agent));
	/** The root svg sized to its box: the kit's icon box sizes only a direct svg child, and SchemeAsset wraps the mark
	 * in display:contents spans, so an unsized mark collapsed to 0x0 (codeatlasdev/ui#297). */
	const fill = (svg: string | undefined) =>
		svg?.replace(/<svg\b[^>]*>/, (tag) =>
			tag.replace(/\s(width|height)="[^"]*"/g, "").replace(/^<svg/, '<svg width="100%" height="100%"'),
		);
</script>

{#snippet light()}{@html fill(logo?.light)}{/snippet}
{#snippet dark()}{@html fill(logo?.dark ?? logo?.light)}{/snippet}

{#if logo}
	{#if bare}
		<SchemeAsset {light} {dark} />
	{:else}
		<span
			class="logo"
			style:width="{size}px"
			style:height="{size}px"
			aria-hidden="true"
			data-contrast-decorative
		>
			<SchemeAsset {light} {dark} />
		</span>
	{/if}
{:else if bare}
	<Bot />
{:else}
	<Bot {size} />
{/if}

<style>
	.logo {
		display: inline-flex;
		flex-shrink: 0;
	}
	.logo :global(svg) {
		width: 100%;
		height: 100%;
	}
</style>
