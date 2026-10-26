<script lang="ts" generics="T">
	/**
	 * Windowed list: only the items near the viewport are in the DOM, so a 20k-line diff, a 2 h chat or a 5k-line
	 * build log costs the same as a screenful. Heights are estimated until a row is on screen, then measured (one
	 * ResizeObserver, one rAF flush per burst). `stickBottom` follows the tail while the reader is at the end and lets go
	 * the moment they scroll up. Logs.svelte keeps its own fixed-row engine (it needs wrap math and a time-sliced view).
	 */
	import { onMount, tick, untrack, type Snippet } from "svelte";
	import { Heights, windowFor } from "#lib/virtual.ts";

	let {
		items,
		key,
		estimate = 24,
		overscan = 6,
		stickBottom = false,
		class: cls = "",
		item,
		header,
		footer,
		scroller = $bindable(),
		onfollow,
		firstVisible = $bindable(0),
	}: {
		items: readonly T[];
		/** Stable identity per item: measures survive prepends/trims, and the keyed `each` reuses rows. */
		key: (it: T, i: number) => string;
		/** Height used until a row is measured. */
		estimate?: number;
		overscan?: number;
		/** Keep the end in view while the reader is at the end. */
		stickBottom?: boolean;
		class?: string;
		item: Snippet<[T, number]>;
		/** Scrolls with the content, above the first item. */
		header?: Snippet;
		/** Scrolls with the content, below the last item. */
		footer?: Snippet;
		scroller?: HTMLDivElement;
		/** The follow state changed (true = pinned to the end). */
		onfollow?: (following: boolean) => void;
		/** Index of the first item at the top edge of the viewport (for sticky section headers). */
		firstVisible?: number;
	} = $props();

	const heights = new Heights(untrack(() => estimate));
	/** Bumped when heights changed, to re-derive the window. */
	let rev = $state(0);
	let scrollTop = $state(0);
	let viewH = $state(600);
	let following = $state(true);
	/** True while a programmatic jump to the end is in flight, so its scroll event is not read as the user scrolling. */
	let pinned = false;
	let lastTop = 0;

	// `keys` is O(n) per change of `items`; the engine needs it to carry measures across prepends/trims.
	let keys = $derived(items.map(key));
	let win = $derived.by(() => {
		void rev;
		heights.resize(items.length, keys);
		return windowFor(heights, scrollTop, viewH, overscan);
	});
	let total = $derived.by(() => {
		void rev;
		void win;
		return heights.total;
	});
	let offset = $derived.by(() => {
		void rev;
		void total;
		return heights.top(win.first);
	});
	let slice = $derived(items.slice(win.first, win.last));
	$effect(() => {
		void rev;
		firstVisible = Math.min(heights.at(scrollTop), Math.max(0, items.length - 1));
	});

	function toEnd() {
		const el = scroller;
		if (!el) return;
		pinned = true;
		el.scrollTop = el.scrollHeight;
		scrollTop = el.scrollTop;
		lastTop = el.scrollTop;
		queueMicrotask(() => (pinned = false));
	}

	function setFollowing(v: boolean) {
		if (following === v) return;
		following = v;
		onfollow?.(v);
	}

	function onscroll() {
		const el = scroller;
		if (!el || pinned) return;
		const st = el.scrollTop;
		const up = st < lastTop - 1;
		lastTop = st;
		scrollTop = st;
		if (!stickBottom) return;
		if (el.scrollHeight - st - el.clientHeight < 24) setFollowing(true);
		else if (up) setFollowing(false);
	}

	// Content grew (items appended, or a measure above the end moved things): stay on the end when following.
	$effect(() => {
		void items.length;
		void total;
		if (stickBottom && untrack(() => following)) void tick().then(toEnd);
	});

	// ── measurement ──
	const els = new Map<Element, number>();
	let ro: ResizeObserver | undefined;
	let queued = false;

	function flush() {
		queued = false;
		let moved = false;
		for (const [el, i] of els) {
			const h = (el as HTMLElement).offsetHeight;
			if (h > 0 && heights.set(i, h)) moved = true;
		}
		if (moved) rev++;
	}

	function schedule() {
		if (queued) return;
		queued = true;
		requestAnimationFrame(flush);
	}

	/** Svelte action: register a rendered row under its absolute index. */
	function measure(node: HTMLElement, i: number) {
		els.set(node, i);
		ro?.observe(node);
		schedule();
		return {
			update(next: number) {
				els.set(node, next);
				schedule();
			},
			destroy() {
				els.delete(node);
				ro?.unobserve(node);
			},
		};
	}

	onMount(() => {
		ro = new ResizeObserver(schedule);
		for (const el of els.keys()) ro.observe(el);
		return () => ro?.disconnect();
	});

	/** Bring item `i` to the middle (or the top) of the view and stop following. */
	export async function scrollToIndex(i: number, align: "center" | "start" = "center") {
		const el = scroller;
		if (!el || i < 0) return;
		setFollowing(false);
		el.scrollTop = Math.max(0, heights.top(i) - (align === "center" ? viewH / 2 : 0));
		scrollTop = el.scrollTop;
		await tick();
	}

	/** Follow the tail again and jump to it once the pending render landed. */
	export async function scrollToEnd() {
		setFollowing(true);
		await tick();
		toEnd();
	}

	/** Is the reader pinned to the end? A flag kept from scroll events: no layout read. */
	export const isFollowing = () => following;
</script>

<div class="vl {cls}" bind:this={scroller} bind:clientHeight={viewH} {onscroll}>
	{@render header?.()}
	<div class="vl-space" style:height="{total}px">
		<div class="vl-win" style:transform="translateY({offset}px)">
			{#each slice as it, j (keys[win.first + j])}
				<div class="vl-row" use:measure={win.first + j}>{@render item(it, win.first + j)}</div>
			{/each}
		</div>
	</div>
	{@render footer?.()}
</div>

<style>
	.vl {
		overflow: auto;
		min-height: 0;
		min-width: 0;
	}
	.vl-space {
		position: relative;
		overflow: hidden;
	}
	.vl-win {
		position: absolute;
		inset: 0 0 auto 0;
	}
</style>
