<script lang="ts">
	/**
	 * The content area as a pager: the current space and its previous/next neighbours are mounted
	 * as full-width layers. A switch never mounts the incoming space; it only moves transforms
	 * (`spacePager`, pager.ts) and flips roles (`SpaceLayer`). A neighbour rests parked one page away, laid out and
	 * painted, so a gesture only moves a transform; what keeps it out of reach is in `parkedLayer.ts`. A role flip
	 * changes nothing the layer's subtree inherits (#121), so it restyles no descendant.
	 */
	import { onMount, tick } from "svelte";
	import { spacePager } from "#lib/shell/pager.ts";
	import { engineGo, nativeActive, initNativeSwipe, pushZones } from "#lib/shell/nativeSwipe.ts";
	import { guardFocus } from "#lib/shell/parkedLayer.ts";
	import { offendingLayers } from "#lib/shell/layerInvariant.ts";
	import { reconcileSoon } from "#lib/native/surface.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { neighbour } from "#lib/shell/space.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import SpaceLayer from "./SpaceLayer.svelte";
	import SpaceSlot from "./SpaceSlot.svelte";

	let root: HTMLElement | undefined = $state();
	/** A gesture or spring is running: natives parked, the ids in `kept` stay mounted. */
	let moving = $state(false);
	let kept = $state<string[]>([]);
	const layers: Record<string, HTMLElement> = {};

	/** Which side a layer parks on at rest: the previous neighbour (wrap included) on the left, the rest on the right. */
	const side = (id: string) =>
		id === neighbour(shell.spaceIds, shell.activeId, -1) &&
		id !== neighbour(shell.spaceIds, shell.activeId, 1)
			? "prev"
			: "next";

	/** The mounted window (`shell.mounted`: active ± 1) + whatever a running gesture still needs. */
	let ids = $derived.by(() => {
		if (!shell.activeId) return [];
		const list = [...shell.mounted];
		for (const k of kept) if (k && !list.includes(k)) list.push(k);
		return list;
	});

	function layer(node: HTMLElement, id: string) {
		layers[id] = node;
		return {
			destroy() {
				if (layers[id] === node) delete layers[id];
			},
		};
	}

	/**
	 * After a commit or a gesture's rest the native views are reconciled (`lib/native/surface.ts`). Debug builds: no layer but the current one may be on screen (ghost
	 * frames, docs/postmortems/ghost-frames.md). Checked after the next frame; reported as a console error.
	 */
	function checkLayers() {
		// Native views are above every layer: whatever the switch did, none may stay up without an owner.
		reconcileSoon({ ready: ws.loaded });
		if (!("__ATLAS_DEBUG__" in window)) return;
		requestAnimationFrame(() => {
			if (!root) return;
			const bad = offendingLayers(root, spacePager.kept);
			if (bad.length)
				console.error("[layers] non-current layer on screen:", bad.map((el) => el.dataset.space).join(","));
		});
	}

	onMount(() => {
		const detach = spacePager.attach({
			layer: (id) => layers[id],
			width: () => root?.clientWidth ?? 0,
			ids: () => shell.spaceIds,
			active: () => shell.activeId,
			commit: (id) => {
				shell.commitSpace(id);
				checkLayers();
			},
			moving(on, keep) {
				kept = on ? (keep ?? []) : [];
				moving = on;
				if (!on) checkLayers();
			},
			flush: () => tick(),
			engineGo: (dir) => engineGo("space", dir),
		});
		const unguard = root ? guardFocus(root) : () => {};
		void initNativeSwipe();
		return () => {
			unguard();
			detach();
		};
	});

	// Which neighbours exist depends on the active space (no wrap): the sidebar's zone must say so.
	$effect(() => {
		void [shell.spaceIds, shell.activeId, shell.mounted];
		if (nativeActive()) pushZones();
	});
</script>

<div class="pager" bind:this={root} class:moving>
	{#each ids as id (id)}
		{@const current = id === shell.activeId}
		<SpaceLayer {id} {current} side={side(id)} vars={shell.layerVars(id)} {layer}>
			<SpaceSlot {id} {current} {moving} />
		</SpaceLayer>
	{/each}
</div>

<style>
	.pager {
		position: relative;
		flex: 1;
		min-height: 0;
		min-width: 0;
		overflow: hidden;
	}
	/* Inline `transform` (pager.ts) wins over the parked offset while a switch runs. */
	.pager.moving :global(.space-layer) {
		will-change: transform;
	}
</style>
