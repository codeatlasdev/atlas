<script lang="ts">
	// Test host: the editor status bar as Editor.svelte builds it, language-server item included.
	import StatusBar from "#lib/editor/StatusBar.svelte";
	import LspStatus from "#lib/lsp/LspStatus.svelte";

	let { input, doc }: { input: any; doc: any } = $props();
</script>

<StatusBar {input} onaction={() => {}}>
	{#snippet lsp()}<LspStatus {doc} />{/snippet}
</StatusBar>
