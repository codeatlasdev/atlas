<script lang="ts">
	import { agentLogo } from "#lib/agentLogos.ts";

	let {
		agentId,
		todayTokens = 0,
		todayCost = 0,
		limitPct = null,
		resetsAt = null,
	}: {
		agentId: string;
		todayTokens?: number;
		todayCost?: number;
		limitPct?: number | null;
		resetsAt?: string | null;
	} = $props();

	const logo = $derived(agentLogo(agentId));

	function fmtTokens(n: number): string {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
		if (n >= 1000) return `${(n / 1000).toFixed(0)}k`;
		return String(n);
	}

	function fmtResetsAt(iso: string): string {
		const d = new Date(iso);
		return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
	}
</script>

{#if logo && (todayTokens > 0 || todayCost > 0)}
	<div class="meter" title={agentId}>
		<span class="meter-logo">{@html logo.dark ?? logo.light}</span>
		<div class="meter-stats">
			<span class="meter-tokens">{fmtTokens(todayTokens)}</span>
			{#if todayCost > 0}
				<span class="meter-cost">${todayCost.toFixed(2)}</span>
			{/if}
			{#if resetsAt}
				<span class="meter-reset">resets {fmtResetsAt(resetsAt)}</span>
			{/if}
		</div>
		{#if limitPct !== null}
			<div
				class="meter-ring"
				role="progressbar"
				aria-valuenow={limitPct}
				aria-valuemin={0}
				aria-valuemax={100}
			>
				<svg viewBox="0 0 20 20" width="20" height="20">
					<circle cx="10" cy="10" r="8" fill="none" stroke="var(--ca-outline-subtle)" stroke-width="2" />
					<circle
						cx="10"
						cy="10"
						r="8"
						fill="none"
						stroke={limitPct > 80 ? "var(--ca-semantic-warning, #e67700)" : "var(--ca-brand-main)"}
						stroke-width="2"
						stroke-dasharray="{(limitPct / 100) * 50.27} 50.27"
						stroke-linecap="round"
						transform="rotate(-90 10 10)"
					/>
				</svg>
			</div>
		{/if}
	</div>
{/if}

<style>
	.meter {
		display: flex;
		align-items: center;
		gap: 4px;
		cursor: default;
	}
	.meter-logo {
		width: 14px;
		height: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
	}
	.meter-logo :global(svg) {
		width: 14px;
		height: 14px;
	}
	.meter-stats {
		display: flex;
		flex-direction: column;
		gap: 0;
	}
	.meter-tokens {
		font-size: 10px;
		font-weight: 600;
		color: var(--ca-text-primary);
		line-height: 1.2;
	}
	.meter-cost {
		font-size: 9.5px;
		color: var(--ca-text-secondary);
		line-height: 1.2;
	}
	.meter-reset {
		font-size: 9px;
		color: var(--ca-text-secondary);
		line-height: 1.2;
	}
	.meter-ring {
		flex: none;
	}
</style>
