<script lang="ts">
	/**
	 * One pager layer: a root that moves (transform, `will-change`, containment) around the space's content.
	 *
	 * A role flip must change nothing the subtree inherits (#121). `inert`, `pointer-events` and `visibility` all
	 * inherit, so changing one on the root (or on a wrapper: `inert` costs the same there) makes Chromium recompute
	 * the style of every descendant: 57 ms for a layer of 23k nodes, per layer, 286 ms for a switch with a 400-line
	 * diff in each of five spaces. The role therefore drives only `transform`, `will-change` and `contain` (none
	 * inherits; a flip measured 0 ms) and `aria-hidden` (0 ms). A resting neighbour is out of reach because it is
	 * translated out of the pager's clip, `aria-hidden`, and guarded for focus and swipe zones by `parkedLayer.ts`.
	 */
	import type { Snippet } from "svelte";

	let {
		id,
		current,
		side,
		vars,
		layer,
		children,
	}: {
		id: string;
		current: boolean;
		/** Where a neighbour parks at rest. Ignored for the current layer. */
		side: "prev" | "next";
		vars: { accent: string; text: string; onAccent: string; tint: string };
		/** `use:layer` of the pager: registers the root so `pager.ts` can move it. */
		layer: (node: HTMLElement, id: string) => { destroy(): void };
		children: Snippet;
	} = $props();
</script>

<div
	class="space-layer"
	style:--space-accent={vars.accent}
	style:--space-accent-text={vars.text}
	style:--space-on-accent={vars.onAccent}
	style:--space-tint={vars.tint}
	data-space={id}
	data-role={current ? "current" : "near"}
	data-side={current ? undefined : side}
	use:layer={id}
>
	<div class="space-body">
		{@render children()}
	</div>
</div>

<style>
	/* Layers are stacked, each the size of the pager; position comes from `translate3d`, written
	   by pager.ts only while a switch runs. At rest the current layer has no transform at all. */
	.space-layer {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	/* The body is the layer's whole content box: what used to be the root's children, laid out the same. */
	.space-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		min-width: 0;
	}
	/* At rest a neighbour is parked one page away, laid out and composited but out of reach (`aria-hidden`, focus bounced by
	   `parkedLayer.ts`, translated out of the pager's clip), so a gesture only has to move a transform: no layout or
	   first paint at gesture start. Only non-inherited properties change with the role. */
	.space-layer[data-role="near"] {
		contain: layout paint;
		will-change: transform;
		transform: translate3d(100%, 0, 0);
	}
	.space-layer[data-side="prev"] {
		transform: translate3d(-100%, 0, 0);
	}
</style>
