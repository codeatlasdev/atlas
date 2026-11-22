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
	import {
		cardState,
		channelCopy,
		channelName,
		confirmCopy,
		DEFAULT_CHANNEL,
		stageNote,
		type Channel,
	} from "#lib/updates.ts";
	import ReleaseTrain from "./ReleaseTrain.svelte";
	import { m } from "#lib/i18n/m.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import type { Snippet } from "svelte";

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
	let stage = $derived(u.channelState ? stageNote(u.channelState) : "");
	let pendingLabel = $derived(pendingCard ? channelCopy(pendingCard.view, "label") : "");

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
		<h3 id="chan-title">{m.settings_registry_general_update_channel_label()}</h3>
		<Tooltip content={m.settings_updates_channel_tooltip()} side="top">
			<Info size={13} aria-label={m.settings_updates_channel_about_aria()} />
		</Tooltip>
		{#if u.channelsLoading}<span class="busy"
				><Spinner size={11} /> {m.crew_settings_browsers_row_checking()}</span
			>{/if}
	</div>

	{#if stage}<p class="stage">{stage}</p>{/if}

	{#if u.channelState?.env_override}
		<p class="note" role="status">
			<Rich
				parts={m.settings_updates_channel_env_forced.parts({ channel: channelName(u.channelState.channel) })}
				tags={{ b: bold, code: mono }}
			/>
		</p>
	{/if}

	{#if u.channelsError}
		<div class="offline" role="status">
			<WifiOff size={13} />
			<span>{m.settings_updates_channel_offline()}</span>
			<Button variant="ghost" size="sm" disabled={u.channelsLoading} onclick={() => u.loadChannels()}>
				{#snippet leading()}<RefreshCw size={12} />{/snippet}
				{m.common_action_retry()}
			</Button>
		</div>
	{/if}

	{#if cards.length === 0}
		<div class="skeleton" aria-busy="true" aria-label={m.settings_updates_channel_loading_aria()}>
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
						aria-label={m.settings_registry_general_update_channel_label()}
						size="sm"
						disabled={locked}
						value={shown}
						items={cards.map((c) => ({ value: c.view.channel, label: channelCopy(c.view, "label") }))}
						onchange={(v) => pick(v as Channel)}
					/>
				</div>
				{#if shownCard}
					<p class="promise" id="chan-desc">
						<span class="tag tone-{shownCard.tone}">{channelCopy(shownCard.view, "risk_label")}</span>
						{#if shownCard.installed}<span class="tag plain">{m.settings_updates_channel_installed()}</span
							>{/if}
						{channelCopy(shownCard.view, "promise")}
					</p>
					<p
						class="offer"
						class:newer={shownCard.view.action === "update"}
						class:older={shownCard.view.action === "downgrade"}
					>
						{#if u.channelsLoading && !shownCard.view.offered}<Spinner size={10} />{/if}
						{shownCard.offer}
						<span class="cad">· {channelCopy(shownCard.view, "cadence")}</span>
					</p>
					{#if shownCard.view.alpha_era_note}<p class="era">
							{channelCopy(shownCard.view, "alpha_era_note")}
						</p>{/if}
				{/if}
			</div>
			<div class="art"><ReleaseTrain {followed} {pending} /></div>
		</div>
	{/if}

	{#if pendingCard && confirm}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			class="confirm"
			role="group"
			aria-label={m.settings_updates_channel_switch_to_aria({ label: pendingLabel })}
			onkeydown={onkey}
		>
			<p class="ctext">{confirm.text}</p>
			{#if confirm.downgrade}
				<Alert severity="warning" title={m.settings_updates_channel_older_title({ label: pendingLabel })}>
					{m.settings_updates_channel_older_body({ label: pendingLabel, version: confirm.downgrade.version })}
					{confirm.downgrade.warning}
				</Alert>
			{/if}
			<div class="actions">
				<Button variant="ghost" size="sm" disabled={saving} onclick={() => (pending = null)}
					>{m.common_action_cancel()}</Button
				>
				{#if confirm.downgrade}
					<Button variant="glass" size="sm" disabled={saving || !!u.running} onclick={downgrade}>
						{#snippet leading()}<TriangleAlert size={13} />{/snippet}
						{confirm.downgrade.label}
					</Button>
				{/if}
				<Button size="sm" disabled={saving} onclick={apply}>
					{#snippet leading()}{#if saving}<Spinner size={12} />{/if}{/snippet}
					{m.settings_updates_channel_follow({ label: pendingLabel })}
				</Button>
			</div>
		</div>
	{/if}
</section>

{#snippet bold(content: Snippet)}<b>{@render content()}</b>{/snippet}
{#snippet mono(content: Snippet)}<code>{@render content()}</code>{/snippet}

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
		height: 18px;
		font-size: 10.5px;
		font-weight: 600;
		line-height: 1;
		padding: 0 7px;
		margin-right: 4px;
		border-radius: 999px;
		white-space: nowrap;
		vertical-align: middle;
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
	/* One sentence: the offer and its cadence are inline text, so they wrap as a sentence (a flex row made them two
	   columns that each broke on its own and orphaned the "·"). The spinner is the only inline-flex item. */
	.offer {
		margin: 0;
		font-size: 11.5px;
		font-weight: 600;
		line-height: 1.5;
		color: var(--ca-text-secondary);
		text-wrap: pretty;
	}
	.offer :global(.ca-spinner) {
		display: inline-block;
		vertical-align: -1px;
		margin-right: 6px;
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
