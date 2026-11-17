<script lang="ts">
	/** A stand-in for a heavy child component (it loads the whole kit or a native layer): renders a marker element. */
	let { name }: { name: string } = $props();
</script>

<span data-stub={name}></span>
