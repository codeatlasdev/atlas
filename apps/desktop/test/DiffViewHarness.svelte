<script lang="ts">
	// Test host for a DiffView fed the way TabGroupView feeds it: through the real `Lazy`, with `props` an object literal
	// built in the template (so it is a NEW object whenever anything it reads changes). A space switch changes `focused`.
	import Lazy from "#lib/components/Lazy.svelte";

	let {
		worktreeId,
		expose,
	}: {
		worktreeId: string;
		expose: (set: { focused(v: boolean): void }) => void;
	} = $props();

	let focused = $state(false);
	expose({ focused: (v) => (focused = v) });
</script>

<Lazy load={() => import("#lib/components/DiffView.svelte")} props={{ worktreeId, focused }} />
