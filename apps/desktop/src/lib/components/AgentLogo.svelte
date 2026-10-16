<script lang="ts">
	/** The official logo of a coding agent, or the generic robot for an agent without one. See `lib/agent-logos/README.md`. */
	import { Bot } from "@lucide/svelte";
	import { agentLogo } from "#lib/agentLogos.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";

	let { agent, size = 14 }: { agent: string | undefined; size?: number } = $props();

	const logo = $derived(agentLogo(agent));
	const svg = $derived(logo && themes.current?.appearance === "dark" && logo.dark ? logo.dark : logo?.light);
</script>

{#if svg}
	<span
		class="logo"
		style:width="{size}px"
		style:height="{size}px"
		aria-hidden="true"
		data-contrast-decorative>{@html svg}</span
	>
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
