<script lang="ts">
	import type { Bot } from "#lib/stores/bots.svelte.ts";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";
	let {
		bot,
		active = false,
		onclick,
	}: {
		bot: Bot;
		active?: boolean;
		onclick?: () => void;
	} = $props();

	const faceState = $derived<FaceState>(
		bot.state === "needs_you" ? "needs_you" : bot.state === "working" ? "working" : "idle",
	);

	function fmtTime(ts?: number): string {
		if (!ts) return "";
		const d = new Date(ts * 1000);
		const now = new Date();
		if (d.toDateString() === now.toDateString()) {
			return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
		}
		return d.toLocaleDateString([], { month: "short", day: "numeric" });
	}
</script>

<button class="row" class:active {onclick}>
	<BotFace seed={bot.id} look={bot.look} state={faceState} size={36} />
	<div class="row-body">
		<div class="row-top">
			<span class="name">{bot.name}</span>
			<span class="time">{fmtTime(bot.last?.ts)}</span>
		</div>
		<div class="row-bottom">
			<span class="preview">{bot.last?.text ?? "No tasks yet"}</span>
			{#if bot.state === "needs_you"}
				<span class="badge" aria-label="Needs your attention"></span>
			{/if}
		</div>
	</div>
</button>

<style>
	.row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		border-radius: 0;
		cursor: default;
		transition: background 80ms;
	}
	.row:hover {
		background: var(--ca-vibrancy-fill);
	}
	.row.active {
		background: color-mix(in oklab, var(--ca-brand-main) 15%, transparent);
	}
	/* BotFace SVG replaces .avatar; keep flex sizing consistent */
	.row-body {
		flex: 1;
		min-width: 0;
	}
	.row-top {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		margin-bottom: 2px;
	}
	.name {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}
	.time {
		font-size: 11px;
		color: var(--ca-text-secondary);
		flex: none;
	}
	.row-bottom {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 6px;
	}
	.preview {
		font-size: 12px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
	}
	.badge {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-semantic-warning);
		flex: none;
	}
</style>
