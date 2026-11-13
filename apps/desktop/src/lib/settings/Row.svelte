<script lang="ts">
	import type { Snippet } from "svelte";

	let {
		label,
		description,
		id,
		stack = false,
		children,
		lead,
		below,
	}: {
		label: string;
		description?: string;
		/** Search result id; the matching row is scrolled into view and flashed. */
		id?: string;
		/** Control under the text instead of beside it (wide controls). */
		stack?: boolean;
		children?: Snippet;
		/** Icon or logo before the label (an agent, a device). */
		lead?: Snippet;
		/** Full-width content under the row (previews, notices). */
		below?: Snippet;
	} = $props();
</script>

<div class="row" class:stack data-setting={id}>
	<div class="main">
		<div class="ident">
			{#if lead}<span class="lead">{@render lead()}</span>{/if}
			<div class="text">
				<span class="label">{label}</span>
				{#if description}<span class="desc">{description}</span>{/if}
			</div>
		</div>
		{#if children}<div class="control">{@render children()}</div>{/if}
	</div>
	{#if below}<div class="below">{@render below()}</div>{/if}
</div>

<style>
	.row {
		padding: 13px 16px;
		min-width: 0;
	}
	.main {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
	}
	/* The icon and the text are one group: the free width of the row goes between the group and the control, never
	   between an icon and the name it belongs to. */
	.ident {
		display: flex;
		align-items: center;
		gap: 12px;
		flex: 1;
		min-width: 0;
	}
	.lead {
		display: grid;
		place-items: center;
		flex-shrink: 0;
		width: 28px;
		height: 28px;
		color: var(--ca-text-secondary);
	}
	.stack .main {
		flex-direction: column;
		align-items: stretch;
		gap: 10px;
	}
	.stack .text {
		flex: 1;
		max-width: none;
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 3px;
		flex: 0 1 auto;
		min-width: 0;
		max-width: 62ch;
	}
	.label {
		font-size: 13px;
		font-weight: 550;
		color: var(--ca-text-primary);
	}
	.desc {
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.control {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
		min-width: 0;
	}
	.stack .control {
		flex-shrink: 1;
		flex-wrap: wrap;
		justify-content: flex-start;
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
