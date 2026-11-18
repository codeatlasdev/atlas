<script lang="ts">
	/** What one phone did lately (`audit.log`, newest first). Mounted when the owner opens it, so it reads fresh. */
	import { Button, Spinner } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import type { Activity } from "#lib/settings/mobile.ts";
	import { describeActivity } from "#lib/settings/mobileText.ts";
	import { dateTime, relativeTime } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	let { nodeId, now }: { nodeId: string; now: number } = $props();

	let entries = $state<Activity[] | null>(null);
	let error = $state<string | null>(null);

	async function load() {
		error = null;
		try {
			entries = (await hostCall<{ activity: Activity[] }>("mobile_activity", { node_id: nodeId, limit: 20 }))
				.activity;
		} catch (e) {
			error = errText(e);
		}
	}

	$effect(() => {
		void load();
	});
</script>

<div class="activity" aria-label={m.mobile_phone_activity_label()}>
	{#if error}
		<p class="note">
			{m.mobile_phone_activity_error({ error })}
			<Button size="sm" variant="ghost" onclick={load}>{m.mobile_phone_activity_try_again()}</Button>
		</p>
	{:else if entries === null}
		<p class="note"><Spinner size={11} /> {m.mobile_phone_activity_reading()}</p>
	{:else if entries.length === 0}
		<p class="note">{m.mobile_phone_activity_empty()}</p>
	{:else}
		<ul>
			{#each entries as e, i (i)}
				<li>
					<span class="what">{describeActivity(e)}</span>
					<time datetime={new Date(e.ts * 1000).toISOString()} title={dateTime(e.ts * 1000)}>
						{relativeTime(e.ts * 1000, now)}
					</time>
				</li>
			{/each}
		</ul>
		<p class="foot">{m.mobile_phone_activity_footer({ count: entries.length })}</p>
	{/if}
</div>

<style>
	.activity {
		margin-top: 4px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		padding: 6px 4px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	li {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		padding: 5px 10px;
		font-size: 12px;
	}
	.what {
		color: var(--ca-text-primary);
	}
	time {
		flex: none;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	.note,
	.foot {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		padding: 6px 10px;
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.foot {
		font-size: 11.5px;
	}
</style>
