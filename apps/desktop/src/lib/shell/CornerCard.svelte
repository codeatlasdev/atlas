<script lang="ts">
	/**
	 * The one card shell of the bottom-right corner: a claim in the coordinator while mounted, drawn only when the
	 * plan says so (a suggestion that waits, a toast folded into "+N more" render nothing), moved into the corner
	 * column, placed by its slot, same width, radius and elevation as every other card. Esc inside it calls
	 * `ondismiss`; hovering or focusing it calls `onpause`/`onresume` so a timer never runs out under the pointer.
	 */
	import { untrack, type Snippet } from "svelte";
	import { corner as inCorner } from "./cornerStack.ts";
	import { corner } from "./corner.svelte.ts";
	import { cornerFlow } from "./cornerMotion.ts";
	import type { CardKind } from "./cornerPlan.ts";

	interface Props {
		id: string;
		kind: CardKind;
		/** False for a card that is not a claim but takes a place in the plan itself (the "+N more" row). */
		claim?: boolean;
		role?: "alert" | "status" | "group" | "complementary";
		/** Accessible name; every card has one. */
		label: string;
		ondismiss?: () => void;
		onpause?: () => void;
		onresume?: () => void;
		children: Snippet;
	}

	let { id, kind, claim = true, role, label, ondismiss, onpause, onresume, children }: Props = $props();

	// The claim itself reads the claim list: untracked, or the effect would re-run on every change it makes.
	$effect(() => (claim ? untrack(() => corner.claim(id, kind)) : undefined));

	let slot = $derived(corner.slot(id));

	let hovered = false;
	let focused = false;
	function hold() {
		if (hovered || focused) onpause?.();
		else onresume?.();
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== "Escape" || !ondismiss) return;
		e.stopPropagation();
		ondismiss();
	}
</script>

{#if slot >= 0}
	<div use:inCorner class="slot" data-corner-card={id} style:order={slot}>
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
		<div
			class="card"
			{role}
			aria-label={label}
			data-kind={kind}
			in:cornerFlow|global
			out:cornerFlow|global={{ out: true }}
			{onkeydown}
			onmouseenter={() => ((hovered = true), hold())}
			onmouseleave={() => ((hovered = false), hold())}
			onfocusin={() => ((focused = true), hold())}
			onfocusout={(e) => ((focused = e.currentTarget.contains(e.relatedTarget as Node | null)), hold())}
		>
			{@render children()}
		</div>
	</div>
{/if}

<style>
	.slot {
		display: block;
		width: 100%;
	}
	.card {
		width: 100%;
		box-sizing: border-box;
		border-radius: var(--ca-radius-lg, 14px);
		color: var(--ca-text-primary);
		/* Opaque: it floats over the pager, and a blur over moving content re-renders every frame of a swipe. */
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			var(--ca-shadow-md);
	}
	/* Only what needs you now carries a colour: a tinted hairline, the same on every card. */
	.card[data-kind="alert"] {
		box-shadow:
			0 0 0 0.5px color-mix(in oklab, var(--ca-semantic-error) 45%, var(--ca-outline-subtle)),
			var(--ca-shadow-md);
	}
</style>
