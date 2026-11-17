<script lang="ts">
	// The right dock's shape: a hidden sidebar is armed (`active`) and peeks, a pinned one is a dock and arms nothing.
	import EdgePeek from "#lib/shell/EdgePeek.svelte";
	import { docks } from "#lib/stores/docks.svelte.ts";
</script>

<EdgePeek edge="right" size={340} label="Right sidebar" active={!docks.right}>
	{#snippet children()}<p>panel</p>{/snippet}
</EdgePeek>
