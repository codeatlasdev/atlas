<script lang="ts">
	import { Button, Spinner } from "@codeatlasdev/ui";
	import { Play } from "@lucide/svelte";
	import { namesLabel } from "./overview.ts";
	import { m } from "#lib/i18n/m.ts";

	/** "Start the stack" call-to-action shown while the project is stopped. */
	let {
		names,
		startable,
		busy,
		disabled,
		onstart,
	}: {
		/** Every declared service, in display order. */
		names: string[];
		/** How many of them are not disabled. */
		startable: number;
		busy: boolean;
		disabled: boolean;
		onstart: () => void;
	} = $props();
</script>

<div class="invite">
	<div class="invite-text">
		<b>{m.panel_start_invite_start_the_stack()}</b>
		<span>
			{m.panel_start_invite_starts({ count: startable })}
			<span class="mono">{namesLabel(names)}</span>
		</span>
	</div>
	<Button disabled={busy || disabled} onclick={onstart}>
		{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<Play
					size={12}
					fill="currentColor"
				/>{/if}{/snippet}
		{m.panel_start_invite_start()}
	</Button>
</div>

<style>
	.invite {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 14px 16px;
		border-radius: 12px;
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 9%, var(--ca-surface-raised));
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 35%, transparent);
		animation: arrive 260ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	.invite-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.invite-text b {
		font-size: 14px;
		color: var(--ca-text-primary);
	}
	.invite-text .mono {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	@keyframes arrive {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.invite {
			animation: none;
		}
	}
</style>
