<!-- Usage: token sparkline plus today / 7d / 30d totals. -->
<script lang="ts">
	import UsageSparkline from "#lib/crew/UsageSparkline.svelte";
	import type { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import { fmtTokens } from "#lib/crew/settings/usage.ts";
	import { m } from "#lib/i18n/m.ts";

	let { settings }: { settings: CrewmateSettings } = $props();
</script>

<UsageSparkline data={settings.usageData} />
<div class="usage-totals">
	<span class="usage-stat"
		>{m.crew_settings_usage_section_today({ tokens: fmtTokens(settings.usageTotals.today) })}</span
	>
	<span class="usage-stat"
		>{m.crew_settings_usage_section_7d({ tokens: fmtTokens(settings.usageTotals.week) })}</span
	>
	<span class="usage-stat"
		>{m.crew_settings_usage_section_30d({ tokens: fmtTokens(settings.usageTotals.month) })}</span
	>
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
