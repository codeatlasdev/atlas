<script lang="ts">
	// Stand-in for ProjectIcon in component tests: the real tile reads the theme store (Vite-only `import.meta.glob`).
	let { id }: { id: string } = $props();
</script>

<span data-project-icon={id}></span>
