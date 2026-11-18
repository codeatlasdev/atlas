<script lang="ts">
	import { Chip } from "@codeatlasdev/ui";
	import type { Level } from "#lib/logs/model.ts";
	import { svcColor } from "#lib/logs/svcColor.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		names,
		counts,
		only = $bindable(),
		minLevel = $bindable(),
	}: {
		names: string[];
		/** Error lines per service. */
		counts: Record<string, number>;
		only: string | null;
		minLevel: Level;
	} = $props();
</script>

<div class="bar">
	<div class="chips">
		<Chip size="sm" label={m.logs_filter_bar_all()} selected={!only} onclick={() => (only = null)} />
		{#each names as n (n)}
			<span class="svcchip" style="--svc:{svcColor(n)}">
				<Chip
					size="sm"
					label={counts[n] ? `${n} · ${counts[n] > 999 ? "999+" : counts[n]}` : n}
					variant={counts[n] ? "error" : "default"}
					selected={only === n}
					onclick={() => (only = only === n ? null : n)}
				/>
			</span>
		{/each}
	</div>
	<div class="right">
		<div class="levels">
			{#each ["info", "warn", "error"] as const as lv (lv)}
				<Chip
					size="sm"
					variant={lv === "error" ? "error" : lv === "warn" ? "warning" : "default"}
					label={lv === "info" ? m.logs_filter_bar_all_levels() : `${lv}+`}
					selected={minLevel === lv}
					onclick={() => (minLevel = lv)}
				/>
			{/each}
		</div>
	</div>
</div>

<style>
	.bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	.chips,
	.levels {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
		align-items: center;
	}
	.svcchip :global(.ca-chip::before) {
		content: "";
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--svc);
		margin-right: 5px;
		flex-shrink: 0;
	}
	.right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 6px;
	}
</style>
