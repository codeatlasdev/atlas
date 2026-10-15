<script lang="ts">
	// Test host for Tip: a control that spreads the kit's trigger props onto itself, as every real call site does.
	import Tip from "#lib/components/Tip.svelte";

	let { label, command }: { label: string; command?: string } = $props();
</script>

<Tip {label} {command}>
	{#snippet child(props)}
		<button class="ctl" {...props}>x</button>
	{/snippet}
</Tip>
