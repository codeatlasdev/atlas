<script lang="ts">
	/**
	 * Settings ▸ General ▸ Updates ▸ Update channel: a Stable / Beta / Alpha segmented control, one line on
	 * the selected channel and the release-train diagram. Picking one opens an inline confirmation with what
	 * the switch means; only the confirm persists it (shared with the CLI). A channel that offers an *older*
	 * build never installs it by switching: the panel has a separate, explicit downgrade button.
	 */
	import { Alert, Button, SegmentedControl, Skeleton, Spinner, Tooltip } from "@codeatlasdev/ui";
	import { Info, RefreshCw, TriangleAlert, WifiOff } from "@lucide/svelte";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import { cardState, confirmCopy, DEFAULT_CHANNEL, type Channel } from "#lib/updates.ts";
	import ReleaseTrain from "./ReleaseTrain.svelte";

	let cards = $derived(
		cardState(u.channels, u.info?.app.version, u.channelState?.channel ?? DEFAULT_CHANNEL),
	);
	let locked = $derived(!!u.channelState?.env_override || !!u.running);
	/** The card the reader picked and has not confirmed yet. */
	let pending = $state<Channel | null>(null);
	let saving = $state(false);
	let pendingCard = $derived(cards.find((c) => c.view.channel === pending && !c.following));
	let confirm = $derived(pendingCard ? confirmCopy(pendingCard.view) : null);
	let followed = $derived(u.channelState?.channel ?? DEFAULT_CHANNEL);
	/** The segment shown as selected: the unconfirmed pick, else the channel the app follows. */
	let shown = $derived(pending ?? followed);
	let shownCard = $derived(cards.find((c) => c.view.channel === shown));
	let stageNote = $derived(u.channelState?.stage_note ?? "");

	$effect(() => {
		if (!u.channels.length && !u.channelsLoading && !u.channelsError) void u.loadChannels();
	});

	function pick(c: Channel) {
		if (locked) return;
		pending = c === u.channelState?.channel ? null : c;
	}

	async function apply() {
		if (!pending || saving) return;
		saving = true;
		try {
			if (await u.setChannel(pending)) pending = null;
		} finally {
			saving = false;
		}
	}

	async function downgrade() {
		if (!pending || saving) return;
		saving = true;
		try {
			// Switch first (persisted for the CLI too), then install the older build explicitly.
			if (!(await u.setChannel(pending))) return;
			pending = null;
			await u.downgrade();
		} finally {
			saving = false;
		}
	}

	function onkey(e: KeyboardEvent) {
		if (e.key === "Escape" && pending) {
			e.stopPropagation();
			pending = null;
		}
	}
</script>

<section class="chan" aria-labelledby="chan-title" data-setting="update-channel">
	<div class="head">
		<h3 id="chan-title">Update channel</h3>
		<Tooltip
			content="Alpha builds are for trying what is coming and can break. Beta is closer to ready. Stable only gets releases that met the bar."
			side="top"
		>
			<Info size={13} aria-label="About the channels" />
		</Tooltip>
		{#if u.channelsLoading}<span class="busy"><Spinner size={11} /> Checking…</span>{/if}
	</div>

	{#if stageNote}<p class="stage">{stageNote}</p>{/if}

	{#if u.channelState?.env_override}
		<p class="note" role="status">
			The channel is forced to <b>{u.channelState.channel}</b> by <code>ATLAS_UPDATE_CHANNEL</code> in this environment.
			Unset it to choose here.
		</p>
	{/if}

	{#if u.channelsError}
		<div class="offline" role="status">
			<WifiOff size={13} />
			<span>Could not read what each channel offers right now. You can still pick a channel.</span>
			<Button variant="ghost" size="sm" disabled={u.channelsLoading} onclick={() => u.loadChannels()}>
				{#snippet leading()}<RefreshCw size={12} />{/snippet}
				Retry
			</Button>
		</div>
	{/if}

	{#if cards.length === 0}
		<div class="skeleton" aria-busy="true" aria-label="Loading update channels">
			<Skeleton variant="rounded" height={32} />
			<Skeleton variant="rounded" height={44} />
		</div>
	{:else}
		<div class="body">
			<div class="pick">
				<!-- Escape in the picker (or the confirmation) drops an unconfirmed pick. -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="seg" aria-disabled={locked} onkeydown={onkey}>
					<SegmentedControl
						aria-label="Update channel"
						size="sm"
						disabled={locked}
						value={shown}
						items={cards.map((c) => ({ value: c.view.channel, label: c.view.label }))}
						onchange={(v) => pick(v as Channel)}
					/>
				</div>
				{#if shownCard}
					<p class="promise" id="chan-desc">
						<span class="tag tone-{shownCard.tone}">{shownCard.view.risk_label}</span>
						{#if shownCard.installed}<span class="tag plain">Installed</span>{/if}
						{shownCard.view.promise}
					</p>
					<p
						class="offer"
						class:newer={shownCard.view.action === "update"}
						class:older={shownCard.view.action === "downgrade"}
					>
						{#if u.channelsLoading && !shownCard.view.offered}<Spinner size={10} />{/if}
						{shownCard.offer}
						<span class="cad">· {shownCard.view.cadence}</span>
					</p>
					{#if shownCard.view.alpha_era_note}<p class="era">{shownCard.view.alpha_era_note}</p>{/if}
				{/if}
			</div>
			<div class="art"><ReleaseTrain {followed} {pending} /></div>
		</div>
	{/if}

	{#if pendingCard && confirm}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div class="confirm" role="group" aria-label="Switch to {pendingCard.view.label}" onkeydown={onkey}>
			<p class="ctext">{confirm.text}</p>
			{#if confirm.downgrade}
				<Alert severity="warning" title="{pendingCard.view.label} offers an older build">
					{pendingCard.view.label} has {confirm.downgrade.version}, which is older than what you run.
					Switching alone keeps your build and waits for something newer. {confirm.downgrade.warning}
				</Alert>
			{/if}
			<div class="actions">
				<Button variant="ghost" size="sm" disabled={saving} onclick={() => (pending = null)}>Cancel</Button>
				{#if confirm.downgrade}
					<Button variant="glass" size="sm" disabled={saving || !!u.running} onclick={downgrade}>
						{#snippet leading()}<TriangleAlert size={13} />{/snippet}
						{confirm.downgrade.label}
					</Button>
				{/if}
				<Button size="sm" disabled={saving} onclick={apply}>
					{#snippet leading()}{#if saving}<Spinner size={12} />{/if}{/snippet}
					Follow {pendingCard.view.label}
				</Button>
			</div>
		</div>
	{/if}
</section>

<style>
	.chan {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px;
		border-radius: 12px;
		background: var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.head {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	h3 {
		margin: 0;
		font-size: 13px;
		font-weight: 650;
	}
	.busy {
		margin-left: auto;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.stage,
	.note {
		margin: 0;
		padding: 8px 12px;
		border-radius: 10px;
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-semantic-warning) 9%, transparent);
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
	}
	.note b,
	.note code {
		color: var(--ca-text-primary);
	}
	.offline {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.offline span {
		flex: 1;
	}
	/* Controls on the left, the diagram on the right; the diagram drops under them when narrow. */
	.body {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 14px 24px;
	}
	.pick {
		flex: 1 1 280px;
		min-width: 0;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
	}
	.seg[aria-disabled="true"] {
		opacity: 0.6;
	}
	.art {
		flex: 0 1 250px;
		min-width: 190px;
	}
	.promise {
		margin: 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}
	.tag {
		display: inline-flex;
		align-items: center;
		font-size: 10.5px;
		font-weight: 600;
		padding: 1px 7px;
		margin-right: 4px;
		border-radius: 999px;
		vertical-align: 1px;
	}
	.tag.tone-success,
	.tag.tone-warn,
	.tag.tone-error {
		color: var(--tone);
		background: color-mix(in oklab, var(--tone) 14%, transparent);
	}
	.tone-success {
		--tone: var(--ca-semantic-success);
	}
	.tone-warn {
		--tone: var(--ca-semantic-warning);
	}
	.tone-error {
		--tone: var(--ca-semantic-error);
	}
	.tag.plain {
		color: var(--ca-semantic-info);
		background: color-mix(in oklab, var(--ca-semantic-info) 14%, transparent);
	}
	.offer {
		margin: 0;
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11.5px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.offer .cad {
		font-weight: 400;
	}
	.offer.newer {
		color: var(--ca-brand-main);
	}
	.offer.older {
		color: var(--ca-semantic-warning);
	}
	.era {
		margin: 0;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		font-style: italic;
	}
	.confirm {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px 14px;
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-brand-main) 7%, var(--ca-surface-raised));
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--ca-brand-main) 35%, transparent);
	}
	.ctext {
		margin: 0;
		font-size: 12.5px;
		line-height: 1.45;
	}
	.actions {
		display: flex;
		justify-content: flex-end;
		flex-wrap: wrap;
		gap: 8px;
	}
	.skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
</style>
