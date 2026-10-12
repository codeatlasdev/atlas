<script lang="ts">
	import type { Snippet } from "svelte";

	let {
		label,
		description,
		id,
		stack = false,
		children,
		below,
	}: {
		label: string;
		description?: string;
		/** Search result id; the matching row is scrolled into view and flashed. */
		id?: string;
		/** Control under the text instead of beside it (wide controls). */
		stack?: boolean;
		children?: Snippet;
		/** Full-width content under the row (previews, notices). */
		below?: Snippet;
	} = $props();
</script>

<div class="row" class:stack data-setting={id}>
	<div class="main">
		<div class="text">
			<span class="label">{label}</span>
			{#if description}<span class="desc">{description}</span>{/if}
		</div>
		{#if children}<div class="control">{@render children()}</div>{/if}
	</div>
	{#if below}<div class="below">{@render below()}</div>{/if}
</div>

<style>
	.row {
		padding: 11px 14px;
		min-width: 0;
	}
	.main {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.stack .main {
		flex-direction: column;
		align-items: stretch;
		gap: 8px;
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}
	.label {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}
	.desc {
		font-size: 12px;
		line-height: 1.4;
		color: var(--ca-text-secondary);
	}
	.control {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}
	.stack .control {
		flex-shrink: 1;
	}
	.below {
		margin-top: 10px;
	}
	.row:global(.flash) {
		animation: flash 1.4s ease-out;
	}
	@keyframes flash {
		0%,
		40% {
			background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
		}
	}
</style>
