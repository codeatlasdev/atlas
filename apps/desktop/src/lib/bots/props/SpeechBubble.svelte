<script lang="ts">
	/** When true the typing dots play; when false show plain text content via slot */
	let {
		typing = false,
		size = 1,
		children,
	}: { typing?: boolean; size?: number; children?: import("svelte").Snippet } = $props();
</script>

<!-- Speech bubble with optional typing dots -->
<div class="bubble" style:--s={size}>
	{#if typing}
		<span class="dot d1"></span>
		<span class="dot d2"></span>
		<span class="dot d3"></span>
	{:else if children}
		<div class="content">{@render children()}</div>
	{/if}
	<div class="tail"></div>
</div>

<style>
	.bubble {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: calc(4px * var(--s, 1));
		padding: calc(8px * var(--s, 1)) calc(14px * var(--s, 1));
		background: var(--ca-surface-raised, #fff);
		border: 1.5px solid var(--ca-outline-subtle, #e5e7eb);
		border-radius: calc(14px * var(--s, 1));
		border-bottom-left-radius: calc(4px * var(--s, 1));
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
		min-width: calc(48px * var(--s, 1));
		min-height: calc(28px * var(--s, 1));
		justify-content: center;
	}

	.tail {
		position: absolute;
		bottom: calc(-8px * var(--s, 1));
		left: calc(14px * var(--s, 1));
		width: 0;
		height: 0;
		border-left: calc(8px * var(--s, 1)) solid transparent;
		border-top: calc(8px * var(--s, 1)) solid var(--ca-surface-raised, #fff);
		filter: drop-shadow(0 1px 1px rgba(0, 0, 0, 0.06));
	}

	.dot {
		display: inline-block;
		width: calc(6px * var(--s, 1));
		height: calc(6px * var(--s, 1));
		border-radius: 50%;
		background: var(--ca-text-secondary, #888);
		animation: bounce-dot 1.2s ease-in-out infinite;
	}
	.d2 {
		animation-delay: 0.2s;
	}
	.d3 {
		animation-delay: 0.4s;
	}

	@keyframes bounce-dot {
		0%,
		60%,
		100% {
			transform: translateY(0);
			opacity: 0.5;
		}
		30% {
			transform: translateY(calc(-5px * var(--s, 1)));
			opacity: 1;
		}
	}

	.content {
		font-size: calc(11px * var(--s, 1));
		color: var(--ca-text-primary);
		white-space: nowrap;
	}

	@media (prefers-reduced-motion: reduce) {
		.dot {
			animation: none;
			opacity: 0.6;
		}
	}
</style>
