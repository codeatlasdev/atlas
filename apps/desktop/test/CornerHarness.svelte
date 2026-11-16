<script lang="ts">
	/** Mounts the real corner column and a list of notices the test can change (`set`). */
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import CornerStack from "#lib/shell/CornerStack.svelte";

	type Item = { id: string; kind: "alert" | "toast" | "suggestion"; title: string; ondismiss?: () => void };
	let items = $state<Item[]>([]);
	export function set(next: Item[]) {
		items = next;
	}
</script>

<CornerStack />
{#each items as it (it.id)}
	<CornerNotice id={it.id} kind={it.kind} title={it.title} ondismiss={it.ondismiss} />
{/each}
