<script lang="ts">
	/**
	 * The one bottom-right column every transient card lives in. It is a flex column growing upward from the
	 * corner, so two cards can never cover each other whatever their heights; each card sets its own `order` from
	 * the coordinator's plan (`corner.svelte.ts`, `cornerPlan.ts`): nearest the corner first. The column itself
	 * takes no pointer events, only its cards do. In a window under 520px wide it is a full-width bottom sheet.
	 */
	import { tick } from "svelte";
	import CornerCard from "./CornerCard.svelte";
	import KitToasts from "./KitToasts.svelte";
	import { corner } from "./corner.svelte.ts";
	import { requestFlip } from "./cornerMotion.ts";
	import { MORE_ID } from "./cornerPlan.ts";
	import {
		CORNER_BLEED,
		CORNER_GAP,
		CORNER_ID,
		CORNER_MARGIN,
		CORNER_MARGIN_NARROW,
		CORNER_WIDTH,
	} from "./cornerStack.ts";
	import { m } from "#lib/i18n/m.ts";

	// A new order is a layout jump: glide every card that moved once the DOM has caught up with the plan.
	$effect(() => {
		void corner.plan.order.join("\n");
		void tick().then(requestFlip);
	});

	let more = $derived(corner.plan.moreRow);
	let hidden = $derived(corner.plan.hiddenToasts);
</script>

<div
	id={CORNER_ID}
	style:--corner-margin="{CORNER_MARGIN}px"
	style:--corner-margin-narrow="{CORNER_MARGIN_NARROW}px"
	style:--corner-gap="{CORNER_GAP}px"
	style:--corner-width="{CORNER_WIDTH}px"
	style:--corner-bleed="{CORNER_BLEED}px"
	role="region"
	aria-label={m.shell_chrome_notifications()}
></div>

<KitToasts />

<CornerCard
	id={MORE_ID}
	kind="toast"
	claim={false}
	role="group"
	label={m.shell_chrome_corner_stack_more_notifications()}
>
	<button
		class="more"
		type="button"
		aria-expanded={more === "less"}
		aria-label={more === "less"
			? m.shell_chrome_corner_stack_show_fewer()
			: m.shell_chrome_corner_stack_show_more({ count: hidden })}
		onclick={() => corner.toggleMore()}
	>
		{more === "less"
			? m.shell_chrome_corner_stack_show_less()
			: m.shell_chrome_corner_stack_hidden_count({ count: hidden })}
	</button>
</CornerCard>

<style>
	/* The column scrolls when a window is too short for every card, which would clip the cards' shadows at its
	   edges: it carries a bleed of padding the same size as its negative offset, so the cards sit exactly on the margin. */
	#atlas-corner {
		position: fixed;
		right: calc(var(--corner-margin) - var(--corner-bleed));
		bottom: calc(var(--corner-margin) - var(--corner-bleed));
		z-index: 1500;
		box-sizing: border-box;
		display: flex;
		flex-direction: column-reverse;
		align-items: stretch;
		gap: var(--corner-gap);
		padding: var(--corner-bleed);
		width: min(
			calc(var(--corner-width) + 2 * var(--corner-bleed)),
			calc(100vw - 2 * (var(--corner-margin) - var(--corner-bleed)))
		);
		max-height: calc(100vh - 2 * (var(--corner-margin) - var(--corner-bleed)));
		overflow-y: auto;
		overscroll-behavior: contain;
		scrollbar-width: none;
		pointer-events: none;
	}
	#atlas-corner::-webkit-scrollbar {
		display: none;
	}
	#atlas-corner > :global(*) {
		position: relative;
		pointer-events: auto;
		flex: none;
	}
	/* an empty column must not draw or catch anything */
	#atlas-corner:not(:has(*)) {
		display: none;
	}
	/* a narrow window: a full-width bottom sheet, clear of the home indicator where there is one */
	@media (max-width: 520px) {
		#atlas-corner {
			left: calc(var(--corner-margin-narrow) - var(--corner-bleed));
			right: calc(var(--corner-margin-narrow) - var(--corner-bleed));
			bottom: calc(max(var(--corner-margin-narrow), env(safe-area-inset-bottom)) - var(--corner-bleed));
			width: auto;
			max-height: calc(100vh - 2 * (var(--corner-margin-narrow) - var(--corner-bleed)));
		}
	}
	.more {
		display: block;
		width: 100%;
		padding: 6px 12px;
		border: 0;
		border-radius: var(--ca-radius-lg, 14px);
		background: transparent;
		font: inherit;
		font-size: 11.5px;
		font-weight: 500;
		text-align: center;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.more:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
</style>
