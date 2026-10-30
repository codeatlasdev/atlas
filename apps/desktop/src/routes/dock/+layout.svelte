<script lang="ts">
	// The agent dock is its own webview: no app shell, no workspace, no menu sync.
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import { onMount, type Snippet } from "svelte";
	import Contain from "#lib/errors/Contain.svelte";
	import { startErrorHandler } from "#lib/errors/errors.svelte.ts";

	let { children }: { children: Snippet } = $props();

	// No toast stack in a pill this small: uncaught errors are logged (and deduped) only.
	onMount(() => startErrorHandler("dock"));
</script>

<Contain where="Agent dock" title="The dock hit an error" hint="Open Atlas from the Dock to keep working.">
	{@render children()}
</Contain>
