<script lang="ts">
	/**
	 * The pair card: the three steps, and the one-time code in whichever state it is (none yet, live with a
	 * countdown, expired, could not start). It owns no state: `MobileSettings` starts the code and watches the list,
	 * so the phone that scans it appears below on its own.
	 */
	import { Alert, Button, CopyButton, Progress, Spinner } from "@codeatlasdev/ui";
	import { QrCode, RefreshCw, Smartphone } from "@lucide/svelte";
	import { renderSVG } from "uqr";
	import {
		clock,
		groupCode,
		MOBILE_APP,
		pairUrl,
		secondsLeft,
		type PairStart,
	} from "#lib/settings/mobile.ts";
	import ExtLink from "./ExtLink.svelte";

	let {
		pairing,
		now,
		busy,
		error,
		paired,
		onstart,
	}: {
		pairing: { p: PairStart; at: number } | null;
		now: number;
		busy: boolean;
		/** Why a code could not be made. */
		error: string | null;
		/** The name of the phone that just paired, until the next code. */
		paired: string | null;
		onstart: () => void;
	} = $props();

	const left = $derived(pairing ? secondsLeft(pairing.p, pairing.at, now) : 0);
	const state = $derived(pairing ? (left > 0 ? "live" : "expired") : "idle");
	const stores = $derived(
		[
			{ name: "App Store", url: MOBILE_APP.appStore },
			{ name: "Google Play", url: MOBILE_APP.playStore },
		].filter((s): s is { name: string; url: string } => !!s.url),
	);
</script>

<div class="pair" data-setting="mobile-pair" data-state={state}>
	<div class="stage">
		{#if pairing && state === "live"}
			<div class="qr" aria-label="Pairing QR code" role="img">{@html renderSVG(pairUrl(pairing.p))}</div>
		{:else}
			<div class="qr placeholder">
				{#if state === "expired"}
					<RefreshCw size={26} />
					<strong>Code expired</strong>
					<span>A code works for two minutes.</span>
				{:else}
					<QrCode size={34} />
					<span>Your code appears here</span>
				{/if}
			</div>
		{/if}
	</div>

	<div class="side">
		<ol class="steps">
			<li>
				<span class="n">1</span>
				<div>
					<strong>Get Atlas Mobile</strong>
					{#if stores.length}
						<span class="hint">
							{#each stores as s, i (s.name)}{#if i}
									·
								{/if}<ExtLink href={s.url}>{s.name}</ExtLink>{/each}
						</span>
					{:else}
						<span class="hint">
							Still in testing, not in the stores yet.
							<ExtLink href={MOBILE_APP.docs}>How to get it</ExtLink>
						</span>
					{/if}
				</div>
			</li>
			<li>
				<span class="n">2</span>
				<div>
					<strong>Open it</strong>
					<span class="hint"
						>Tap <em>Scan pairing code</em>, or <em>Pair another computer</em> in its Settings.</span
					>
				</div>
			</li>
			<li>
				<span class="n">3</span>
				<div>
					<strong>Scan the code</strong>
					<span class="hint">The phone appears in <em>Paired phones</em> below the moment it connects.</span>
				</div>
			</li>
		</ol>

		{#if state === "live" && pairing}
			<div class="live">
				<div class="codeline">
					<span class="code ca-selectable" aria-label="One-time code">{groupCode(pairing.p.code)}</span>
					<span class="left" role="timer">Expires in {clock(left)}</span>
				</div>
				<Progress value={left} max={pairing.p.expires_in_s} size="sm" label="Time left on this code" />
				<div class="waiting"><Spinner size={11} /> Waiting for your phone…</div>
				<div class="actions">
					<Button size="sm" variant="outlined" onclick={onstart} loading={busy}>New code</Button>
					<CopyButton
						size="sm"
						variant="ghost"
						label="Copy link"
						value={pairUrl(pairing.p)}
						toast="Pairing link copied. In the app, choose Paste link."
					/>
				</div>
			</div>
		{:else}
			{#if error}
				<Alert severity="error" title="Could not make a code">
					{error}. Try again; if it keeps failing, restart Atlas.
				</Alert>
			{:else if paired && state === "idle"}
				<Alert severity="success" title="{paired} is paired">It is in the list below.</Alert>
			{/if}
			<div class="actions">
				<Button onclick={onstart} loading={busy}>
					{#snippet leading()}<Smartphone size={14} />{/snippet}
					{state === "expired" ? "New code" : paired && !error ? "Pair another phone" : "Show pairing code"}
				</Button>
			</div>
		{/if}
	</div>
</div>

<style>
	.pair {
		display: flex;
		gap: 24px;
		align-items: flex-start;
		padding: 16px;
		flex-wrap: wrap;
	}
	.stage {
		flex: none;
	}
	.qr {
		width: 188px;
		height: 188px;
		box-sizing: border-box;
		padding: 10px;
		border-radius: 10px;
		background: #fff;
	}
	.qr :global(svg) {
		width: 100%;
		height: 100%;
	}
	.placeholder {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 6px;
		text-align: center;
		background: var(--ca-surface-sunken);
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
		color: var(--ca-text-secondary);
		font-size: 12px;
	}
	.placeholder strong {
		color: var(--ca-text-primary);
		font-size: 13px;
	}
	.side {
		flex: 1;
		min-width: 260px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.steps li {
		display: flex;
		gap: 10px;
		align-items: flex-start;
	}
	.steps li > div {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.steps strong {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.hint {
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.n {
		flex: none;
		display: grid;
		place-items: center;
		width: 20px;
		height: 20px;
		border-radius: 50%;
		font-size: 11px;
		font-weight: 650;
		background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
		color: var(--ca-brand-main);
	}
	.live {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding-top: 14px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.codeline {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
	}
	.code {
		font: 600 24px/1 var(--ca-font-mono, monospace);
		letter-spacing: 0.12em;
		color: var(--ca-text-primary);
	}
	.left {
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
	}
	.waiting {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.actions {
		display: flex;
		gap: 8px;
		align-items: center;
	}
</style>
