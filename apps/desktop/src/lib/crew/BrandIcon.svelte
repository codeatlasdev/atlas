<!--
  The real logo of an integration, on a rounded tile like the catalog in the Grok reference.

  Marks come from simple-icons (CC0), so they are the brands' own shapes and colours. A brand that simple-icons does not
  carry (Slack) shows its initial until a mark with a confirmed licence is added.
  Near-black brands (GitHub, Notion) draw in the theme's text colour on a neutral tile, so they read in light and dark.
-->
<script lang="ts">
	import { si1password, siFigma, siGithub, siGoogledocs, siLinear, siNotion } from "simple-icons";

	let { id, size = 36 }: { id: string; size?: number } = $props();

	const MONO: Record<string, { path: string; hex: string }> = {
		github: siGithub,
		linear: siLinear,
		notion: siNotion,
		figma: siFigma,
		"1password": si1password,
		"google-docs": siGoogledocs,
	};

	const mark = $derived(MONO[id]);
	/** Dark marks (#181717, #000000) disappear on a dark tile: draw them in the theme's ink instead. */
	const dark = $derived(mark ? parseInt(mark.hex, 16) < 0x333333 : false);
	const glyph = $derived(Math.round(size * 0.58));
</script>

<span
	class="brand-tile"
	style:width="{size}px"
	style:height="{size}px"
	style:border-radius="{Math.round(size * 0.24)}px"
>
	{#if mark}
		<svg width={glyph} height={glyph} viewBox="0 0 24 24" aria-hidden="true">
			<path d={mark.path} fill={dark ? "currentColor" : `#${mark.hex}`} />
		</svg>
	{:else}
		<!-- No logo with a confirmed licence yet (e.g. Slack): the initial, never a guessed mark. -->
		<span class="initial" style:font-size="{Math.round(size * 0.42)}px">{id.charAt(0).toUpperCase()}</span>
	{/if}
</span>

<style>
	.initial {
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.brand-tile {
		display: inline-grid;
		place-items: center;
		flex: none;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised, var(--ca-surface-sunken));
		border: 0.5px solid var(--ca-outline-subtle);
	}
</style>
