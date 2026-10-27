<script lang="ts">
	/** A small tree that shows where the next worktree is created and what its branch is called. */
	import { worktreePreview, type WorktreePrefs } from "./worktreePreview.ts";

	let { prefs }: { prefs: WorktreePrefs } = $props();
	let p = $derived(worktreePreview(prefs));
	const ROW = 22;
</script>

<figure aria-label="Where the next worktree is created">
	<svg viewBox="0 0 360 {p.path.length * ROW + 6}" role="img" aria-hidden="true">
		{#each p.path as part, i (i + part)}
			{@const y = i * ROW + 14}
			{@const x = i === 0 ? 4 : 4 + i * 16}
			{#if i > 0}
				<path d="M{x - 10} {y - 12} V{y - 3} H{x - 3}" class="line" />
			{/if}
			<rect {x} y={y - 8} width="12" height="9" rx="2" class="dir" class:last={i === p.path.length - 1} />
			<text x={x + 18} {y} class="name" class:last={i === p.path.length - 1}>{part}</text>
		{/each}
	</svg>
	<figcaption>
		New worktrees go here, on the branch <code>{p.branch}</code>.
	</figcaption>
</figure>

<style>
	figure {
		display: flex;
		align-items: center;
		gap: 16px;
		margin: 0;
		padding: 10px 12px;
		border-radius: 9px;
		background: var(--ca-surface-sunken);
	}
	svg {
		flex-shrink: 0;
		width: 250px;
		overflow: visible;
	}
	.line {
		fill: none;
		stroke: var(--ca-outline-default, var(--ca-outline-subtle));
		stroke-width: 1;
	}
	.dir {
		fill: var(--ca-text-hint);
		opacity: 0.55;
	}
	.dir.last {
		fill: var(--ca-brand-main);
		opacity: 1;
	}
	.name {
		font-size: 11.5px;
		font-family: var(--mono);
		fill: var(--ca-text-secondary);
	}
	.name.last {
		fill: var(--ca-text-primary);
		font-weight: 600;
	}
	figcaption {
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	code {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--ca-text-primary);
		word-break: break-all;
	}
</style>
