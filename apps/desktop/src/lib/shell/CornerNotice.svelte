<script lang="ts">
	/**
	 * A message card of the corner: icon, title, optional text, optional actions, a dismiss button. The role follows
	 * the kind: an error (`alert`) is announced at once, a toast (`status`) politely, a suggestion is a named group
	 * that is not announced at all.
	 */
	import { CircleCheck, Info, OctagonAlert, TriangleAlert, X } from "@lucide/svelte";
	import type { Component, Snippet } from "svelte";
	import CornerCard from "./CornerCard.svelte";
	import type { CardKind } from "./cornerPlan.ts";

	type Tone = "neutral" | "error" | "warning" | "success" | "info" | "brand";

	interface Props {
		id: string;
		kind: Exclude<CardKind, "checklist">;
		tone?: Tone;
		title: string;
		/** Plain second line; `children` does the same with markup. */
		body?: string;
		/** Replaces the tone's icon. */
		icon?: Component<{ size?: number; strokeWidth?: number }>;
		/** "×3": how many times the same thing happened. */
		count?: number;
		ondismiss?: () => void;
		dismissLabel?: string;
		onpause?: () => void;
		onresume?: () => void;
		children?: Snippet;
		actions?: Snippet;
	}

	let {
		id,
		kind,
		tone = "neutral",
		title,
		body,
		icon,
		count = 1,
		ondismiss,
		dismissLabel = "Dismiss",
		onpause,
		onresume,
		children,
		actions,
	}: Props = $props();

	const ICONS = { error: OctagonAlert, warning: TriangleAlert, success: CircleCheck, info: Info };
	let Icon = $derived(icon ?? ICONS[tone as keyof typeof ICONS]);
	let role = $derived(kind === "alert" ? "alert" : kind === "toast" ? "status" : "group") as
		"alert" | "status" | "group";
</script>

<CornerCard {id} {kind} {role} label={title} {ondismiss} {onpause} {onresume}>
	<div class="notice" data-tone={tone}>
		{#if Icon}<span class="icon" aria-hidden="true"><Icon size={15} strokeWidth={1.9} /></span>{/if}
		<div class="text">
			<b class="title">{title}</b>
			{#if body}<span class="body">{body}</span>{:else if children}<span class="body"
					>{@render children()}</span
				>{/if}
			{#if actions}<div class="actions">{@render actions()}</div>{/if}
		</div>
		{#if count > 1}<span class="count" aria-hidden="true">×{count}</span>{/if}
		{#if ondismiss}
			<button class="x" type="button" aria-label={dismissLabel} onclick={ondismiss}><X size={13} /></button>
		{/if}
	</div>
</CornerCard>

<style>
	.notice {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 8px 10px 12px;
	}
	.icon {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 7px;
		color: var(--ca-text-secondary);
		background: var(--ca-action-hover);
	}
	[data-tone="error"] .icon {
		color: var(--ca-semantic-error);
		background: color-mix(in oklab, var(--ca-semantic-error) 13%, transparent);
	}
	[data-tone="warning"] .icon {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 13%, transparent);
	}
	[data-tone="success"] .icon {
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--ca-semantic-success) 13%, transparent);
	}
	[data-tone="info"] .icon,
	[data-tone="brand"] .icon {
		color: var(--space-accent, var(--ca-brand-main));
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 13%, transparent);
	}
	.text {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding-top: 3px;
		font-size: 12.5px;
		line-height: 1.35;
	}
	.title {
		font-weight: 600;
		overflow-wrap: anywhere;
	}
	.body {
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		overflow: hidden;
		overflow-wrap: anywhere;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 6px;
	}
	.count {
		flex: none;
		padding-top: 4px;
		font: 11px var(--mono);
		color: var(--ca-text-secondary);
	}
	.x {
		flex: none;
		display: grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.x:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
</style>
