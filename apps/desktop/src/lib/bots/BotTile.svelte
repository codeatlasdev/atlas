<script lang="ts">
	import { crossfade } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { Bot } from "#lib/stores/bots.svelte.ts";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";

	let {
		bot,
		active = false,
		unread = false,
		onclick,
	}: {
		bot: Bot;
		active?: boolean;
		unread?: boolean;
		onclick?: () => void;
	} = $props();

	const faceState = $derived<FaceState>(
		bot.state === "needs_you" ? "needs_you" : bot.state === "working" ? "working" : "idle",
	);

	const [send, receive] = crossfade({
		duration: 200,
		easing: cubicOut,
		fallback(node) {
			return { duration: 150, css: (t) => `opacity: ${t}` };
		},
	});
</script>

<button
	class="bot-tile"
	class:active
	class:unread
	{onclick}
	aria-label="{bot.name}{unread ? ', unread' : ''}"
	aria-pressed={active}
>
	<div class="tile-face-wrap">
		{#if bot.state === "working"}
			<div class="tile-dots" in:receive={{ key: bot.id }} out:send={{ key: bot.id }}>
				<span class="dot"></span>
				<span class="dot"></span>
				<span class="dot"></span>
			</div>
		{:else}
			<div class="tile-face" in:receive={{ key: bot.id }} out:send={{ key: bot.id }}>
				<BotFace seed={bot.id} look={bot.look} state={faceState} size={56} />
			</div>
		{/if}
		<!-- Status badge -->
		{#if bot.state === "needs_you"}
			<span class="badge badge-needs-you" aria-label="Needs you"></span>
		{:else if unread}
			<span class="badge badge-unread" aria-label="Unread"></span>
		{/if}
	</div>
	<span class="tile-name">{bot.name}</span>
</button>

<style>
	.bot-tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		padding: 12px 8px;
		border-radius: 12px;
		background: transparent;
		border: none;
		cursor: default;
		color: var(--ca-text-primary);
		transition: background 150ms;
		width: 88px;
		text-align: center;
	}
	.bot-tile:hover {
		background: var(--ca-vibrancy-fill);
	}
	.bot-tile.active {
		background: color-mix(in oklab, var(--ca-brand-main) 15%, transparent);
	}
	.tile-face-wrap {
		position: relative;
		width: 56px;
		height: 56px;
	}
	.tile-face,
	.tile-dots {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 14px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
	}
	/* Bouncing dots */
	.tile-dots {
		gap: 5px;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		animation: dot-bounce 1.2s ease-in-out infinite;
	}
	.dot:nth-child(2) {
		animation-delay: 0.15s;
	}
	.dot:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot-bounce {
		0%,
		80%,
		100% {
			transform: scale(0.8);
			opacity: 0.5;
		}
		40% {
			transform: scale(1.1);
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot {
			animation: none;
			opacity: 0.8;
		}
	}
	/* Badge */
	.badge {
		position: absolute;
		bottom: -1px;
		right: -1px;
		width: 14px;
		height: 14px;
		border-radius: 50%;
		border: 2px solid var(--ca-surface-sunken);
	}
	.badge-needs-you {
		background: #f59e0b;
	}
	.badge-unread {
		background: var(--ca-brand-main);
	}
	.tile-name {
		font-size: 11px;
		font-weight: 500;
		color: var(--ca-text-primary);
		max-width: 80px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		line-height: 1.2;
	}
</style>
