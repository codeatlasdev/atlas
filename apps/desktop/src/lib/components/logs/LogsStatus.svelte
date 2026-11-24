<script lang="ts">
	import { kbd, modClick } from "#lib/keys.ts";
	import { Kbd, Spinner } from "@codeatlasdev/ui";
	import { formatNumber } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		total,
		ofTotal,
		errors,
		warns,
		filtering,
	}: {
		total: number;
		/** Buffered lines when a filter narrows the view, else null. */
		ofTotal: number | null;
		errors: number;
		warns: number;
		filtering: boolean;
	} = $props();
</script>

<div class="status">
	<span>
		{#if ofTotal === null}
			{m.logs_status_lines({ count: total, amount: formatNumber(total) })}
		{:else}
			{m.logs_status_lines_of_total({
				count: total,
				amount: formatNumber(total),
				total: formatNumber(ofTotal),
			})}
		{/if}
	</span>
	{#if errors}<span class="e">
			{m.logs_status_errors({ count: errors, amount: formatNumber(errors) })}
		</span>{/if}
	{#if warns}<span class="w">
			{m.logs_status_warnings({ count: warns, amount: formatNumber(warns) })}
		</span>{/if}
	{#if filtering}<span class="busy">
			<Spinner size={11} label={m.logs_status_working()} />
			{m.logs_status_filtering()}
		</span>{/if}
	<span class="hint">
		<Kbd>{kbd("cmd-f")}</Kbd>
		{m.logs_status_hint_find()} · <Kbd>{kbd("e")}</Kbd>
		<Kbd>{kbd("w")}</Kbd>
		{m.logs_status_hint_jump()} · {m.logs_status_hint_links({ key: modClick() })} ·
		<Kbd>{kbd("cmd-l")}</Kbd>
		{m.logs_status_hint_logs()}
	</span>
</div>

<style>
	.status {
		display: flex;
		gap: 12px;
		align-items: center;
		padding: 5px 16px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.status .e {
		color: var(--ca-semantic-error);
	}
	.status .w {
		color: var(--ca-semantic-warning);
	}
	.busy {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.hint {
		margin-left: auto;
	}
</style>
