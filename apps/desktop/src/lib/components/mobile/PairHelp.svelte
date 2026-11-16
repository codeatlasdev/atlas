<script lang="ts">
	/** "Pairing doesn't work?": the three causes that account for nearly every failed pairing (docs/mobile.md). */
	import FlatDisclosure from "#lib/components/FlatDisclosure.svelte";
	import { VPN_DOCS, type PairStart } from "#lib/settings/mobile.ts";
	import ExtLink from "./ExtLink.svelte";

	let { pairing }: { pairing: PairStart | null } = $props();
</script>

<div class="help">
	<FlatDisclosure title="Pairing doesn't work?">
		<ul>
			<li>
				<strong>The phone does not need to be on this Wi-Fi.</strong> It connects through an encrypted relay, or
				straight to this computer when both are on the same network.
			</li>
			<li>
				<strong>Keep this computer awake</strong> with Atlas running. A sleeping computer cannot be reached; the
				phone connects again once it wakes.
			</li>
			<li>
				<strong>VPN or Cloudflare WARP on?</strong> Pause it, or exclude your Wi-Fi network from it, then make
				a new code.
				<ExtLink href={VPN_DOCS}>Pairing fails with a VPN or WARP on</ExtLink>
			</li>
		</ul>
		{#if pairing}
			<p class="addrs">
				This code offers {pairing.addrs.length === 0
					? "no direct addresses, so the phone will use the relay"
					: pairing.addrs.length === 1
						? "this address"
						: "these addresses"}:
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
