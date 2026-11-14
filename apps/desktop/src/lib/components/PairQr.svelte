<script lang="ts">
	/**
	 * PairQr — extracted from MobileSettings.svelte.
	 * Renders a QR code + numeric code for Atlas Mobile pairing.
	 */
	import { renderSVG } from "uqr";
	import { pairUrl, secondsLeft, type PairStart } from "#lib/settings/mobile.ts";

	let { pairing, now }: { pairing: { p: PairStart; at: number }; now: number } = $props();

	const left = $derived(secondsLeft(pairing.p, pairing.at, now));
</script>

{#if left > 0}
	<div class="pair">
		<div class="qr" aria-label="Pairing QR code" role="img">{@html renderSVG(pairUrl(pairing.p))}</div>
		<div>
			<div class="code ca-selectable">{pairing.p.code}</div>
			<div class="muted">Expires in {left}s</div>
		</div>
	</div>
{:else}
	<div class="muted">The code expired. Make a new one.</div>
{/if}

<style>
	.pair {
		display: flex;
		gap: 16px;
		align-items: center;
		margin-top: 8px;
	}
	.qr {
		width: 180px;
		height: 180px;
		background: #fff;
		padding: 8px;
		border-radius: 8px;
		flex: none;
	}
	.qr :global(svg) {
		width: 100%;
		height: 100%;
	}
	.code {
		font: 600 22px/1 var(--ca-font-mono, monospace);
		letter-spacing: 0.15em;
	}
	.muted {
		font-size: 11.5px;
		opacity: 0.7;
		margin-top: 4px;
	}
</style>
