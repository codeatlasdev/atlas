<script lang="ts">
	import BotFace from "#lib/bots/BotFace.svelte";

	const STATES = [
		{ state: "idle" as const, label: "idle" },
		{ state: "working" as const, label: "working" },
		{ state: "needs_you" as const, label: "needs you" },
		{ state: "offline" as const, label: "offline" },
	];

	const SEEDS = ["atlas", "codex", "nexus", "rail"];
</script>

<div class="page">
	<h2 class="title">Bot faces — all states</h2>
	<div class="grid">
		{#each STATES as { state, label }, i}
			<div class="cell">
				<BotFace seed={SEEDS[i]} {state} size={80} />
				<span class="lbl">{label}</span>
			</div>
		{/each}
	</div>
</div>

<style>
	.page {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		gap: 24px;
		background: var(--ca-surface-base);
	}
	.title {
		font-size: 14px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		margin: 0;
	}
	.grid {
		display: flex;
		gap: 48px;
		align-items: flex-end;
	}
	.cell {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
	}
	.lbl {
		font-size: 12px;
		color: var(--ca-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
</style>
