<!-- Usage: token sparkline plus today / 7d / 30d totals. -->
<script lang="ts">
	import UsageSparkline from "#lib/crew/UsageSparkline.svelte";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import { fmtTokens } from "#lib/crew/settings/usage.ts";

	let { settings }: { settings: CrewmateSettings } = $props();
</script>

<UsageSparkline data={settings.usageData} />
<div class="usage-totals">
	<span class="usage-stat">Today: {fmtTokens(settings.usageTotals.today)}</span>
	<span class="usage-stat">7d: {fmtTokens(settings.usageTotals.week)}</span>
	<span class="usage-stat">30d: {fmtTokens(settings.usageTotals.month)}</span>
</div>

<style>
	.usage-totals {
		display: flex;
		gap: 12px;
		padding: 4px 0;
	}
	.usage-stat {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
</style>
