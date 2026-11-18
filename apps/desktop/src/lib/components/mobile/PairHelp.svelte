<script lang="ts">
	/** "Pairing doesn't work?": the three causes that account for nearly every failed pairing (docs/mobile.md). */
	import type { Snippet } from "svelte";
	import FlatDisclosure from "#lib/components/FlatDisclosure.svelte";
	import { VPN_DOCS, type PairStart } from "#lib/settings/mobile.ts";
	import ExtLink from "./ExtLink.svelte";
	import { m } from "#lib/i18n/m.ts";
	import Rich from "#lib/i18n/Rich.svelte";

	let { pairing }: { pairing: PairStart | null } = $props();
</script>

{#snippet bold(content: Snippet)}<strong>{@render content()}</strong>{/snippet}

<div class="help">
	<FlatDisclosure title={m.mobile_pair_help_title()}>
		<ul>
			<li><Rich parts={m.mobile_pair_help_wifi.parts()} tags={{ b: bold }} /></li>
			<li><Rich parts={m.mobile_pair_help_awake.parts()} tags={{ b: bold }} /></li>
			<li>
				<Rich parts={m.mobile_pair_help_vpn.parts()} tags={{ b: bold }} />
				<ExtLink href={VPN_DOCS}>{m.mobile_pair_help_vpn_docs()}</ExtLink>
			</li>
		</ul>
		{#if pairing}
			<p class="addrs">
				{m.mobile_pair_help_addrs({ count: pairing.addrs.length })}
				{#each pairing.addrs as a (a)}<code class="ca-selectable">{a}</code>{/each}
			</p>
		{/if}
	</FlatDisclosure>
</div>

<style>
	.help {
		padding: 8px 16px 10px;
	}
	ul {
		margin: 4px 0 8px;
		padding-left: 18px;
		display: flex;
		flex-direction: column;
		gap: 8px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}
	strong {
		color: var(--ca-text-primary);
		font-weight: 600;
	}
	.addrs {
		margin: 0 0 8px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	code {
		display: inline-block;
		margin: 2px 6px 2px 0;
		padding: 1px 6px;
		border-radius: 5px;
		background: var(--ca-surface-sunken);
		font: 11.5px var(--ca-font-mono, monospace);
	}
</style>
