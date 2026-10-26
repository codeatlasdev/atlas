<script lang="ts">
	// Test host for VirtualList: rows are plain 20px divs; `expose` hands the test the list's inputs and handle.
	import VirtualList from "#lib/components/VirtualList.svelte";

	let {
		expose,
		stickBottom = false,
	}: {
		expose: (h: {
			set(n: number): void;
			el(): HTMLDivElement | undefined;
			list(): { isFollowing(): boolean };
		}) => void;
		stickBottom?: boolean;
	} = $props();

	let items = $state<{ id: string }[]>([]);
	let scroller = $state<HTMLDivElement>();
	let list: { isFollowing(): boolean };
	expose({
		set: (n) => (items = Array.from({ length: n }, (_, i) => ({ id: `r${i}` }))),
		el: () => scroller,
		list: () => list,
	});
</script>

<VirtualList bind:this={list} bind:scroller {items} key={(it) => it.id} estimate={20} {stickBottom}>
	{#snippet item(it)}<div class="r" style="height:20px">{it.id}</div>{/snippet}
</VirtualList>
