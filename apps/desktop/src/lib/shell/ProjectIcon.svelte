<script lang="ts">
	import { LucideIcon, lucideTable, type IconNode } from "#lib/lucide.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import { shell } from "#lib/shell/shell.svelte.ts";

	/**
	 * A project's identity tile: the user's emoji / Lucide icon / image, else the repo's own
	 * logo, else a monogram, on a tile tinted with the project's accent. `id` is the repo id.
	 */
	let {
		id,
		size = 20,
		ring = false,
	}: {
		id: string;
		size?: number;
		/** Draw the accent ring (the active space). */
		ring?: boolean;
	} = $props();

	const icon = $derived(identities.iconOf(id));
	let node = $state<IconNode | null>(null);
	$effect(() => {
		const i = icon;
		if (i.kind !== "lucide") return;
		let live = true;
		void lucideTable().then((t) => live && (node = t[i.value] ?? null));
		return () => (live = false);
	});
</script>

<span
	class="tile {icon.kind}"
	class:ring
	style:--c={shell.accent(id)}
	style:--s="{size}px"
	aria-hidden="true"
>
	{#if icon.kind === "emoji"}
		<span class="emoji">{icon.value}</span>
	{:else if icon.kind === "lucide"}
		{#if node}<LucideIcon iconNode={node} size={Math.round(size * 0.62)} strokeWidth={2.1} />{/if}
	{:else if icon.kind === "image"}
		<img src={icon.src} alt="" draggable="false" />
	{:else}
		<span class="mono-text" class:two={icon.text.length > 1}>{icon.text}</span>
	{/if}
</span>

<style>
	.tile {
		display: inline-grid;
		place-items: center;
		flex: none;
		width: var(--s);
		height: var(--s);
		border-radius: calc(var(--s) * 0.3);
		overflow: hidden;
		color: var(--c);
		background: color-mix(in oklab, var(--c) 18%, transparent);
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--c) 40%, transparent);
		transition:
			box-shadow var(--dur) var(--ease),
			background var(--dur) var(--ease);
	}
	.tile.ring {
		box-shadow:
			inset 0 0 0 0.5px color-mix(in oklab, var(--c) 40%, transparent),
			0 0 0 2px color-mix(in oklab, var(--c) 70%, transparent);
	}
	/* Monogram: solid accent so the letter has the contrast the icon-less tile lacks. */
	.tile.monogram {
		color: white;
		background: linear-gradient(160deg, color-mix(in oklab, var(--c) 82%, white), var(--c));
		box-shadow:
			inset 0 0.5px 0 rgb(255 255 255 / 0.3),
			0 0 0 0.5px color-mix(in oklab, var(--c) 60%, black);
	}
	.tile.monogram.ring {
		box-shadow:
			inset 0 0.5px 0 rgb(255 255 255 / 0.3),
			0 0 0 2px color-mix(in oklab, var(--c) 70%, transparent);
	}
	.mono-text {
		font-size: calc(var(--s) * 0.5);
		font-weight: 700;
		line-height: 1;
		letter-spacing: -0.02em;
		text-shadow: 0 0.5px 0 rgb(0 0 0 / 0.2);
	}
	.mono-text.two {
		font-size: calc(var(--s) * 0.42);
	}
	.emoji {
		font-size: calc(var(--s) * 0.62);
		line-height: 1;
	}
	.tile.image {
		background: color-mix(in oklab, var(--ca-text-primary) 7%, transparent);
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.tile.image.ring {
		box-shadow: 0 0 0 2px color-mix(in oklab, var(--c) 70%, transparent);
	}
	img {
		width: 78%;
		height: 78%;
		object-fit: contain;
	}
	.tile.emoji {
		background: color-mix(in oklab, var(--c) 14%, transparent);
	}
</style>
