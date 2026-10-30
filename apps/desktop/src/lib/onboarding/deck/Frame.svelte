<script lang="ts">
	/** A small drawn window: the stage every slide's scene plays on. Pure markup, sized by its parent. */
	import type { Snippet } from "svelte";

	let { title, children }: { title: string; children: Snippet } = $props();
</script>

<div class="frame">
	<div class="bar">
		<span class="dots" aria-hidden="true"><i></i><i></i><i></i></span>
		<span class="title">{title}</span>
	</div>
	<div class="body">{@render children()}</div>
</div>

<style>
	.frame {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		border-radius: 14px;
		overflow: hidden;
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-default),
			0 24px 60px -20px color-mix(in oklab, black 45%, transparent),
			0 2px 6px color-mix(in oklab, black 12%, transparent);
	}
	.bar {
		height: 28px;
		flex: none;
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 12px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.dots {
		display: flex;
		gap: 5px;
	}
	.dots i {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-outline-strong);
	}
	.title {
		font-weight: 600;
	}
	.body {
		position: relative;
		text-align: left;
		flex: 1;
		min-height: 0;
	}
</style>
