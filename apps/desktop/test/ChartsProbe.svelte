<script lang="ts">
	/** A stand-in for the dashboard's charts (the kit's canvas charts need a real browser): records every `names` value it is given. */
	let { names }: { names: string[] } = $props();
	$effect(() => {
		(globalThis as { __chartNames?: string[][] }).__chartNames?.push([...names]);
	});
</script>

<span data-stub="charts"></span>
