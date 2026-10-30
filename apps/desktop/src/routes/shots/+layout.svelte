<script lang="ts">
	// The capture pages (overlay, quick access, HUD, editor) are separate webviews: no app shell, no workspace, no
	// menu sync. The overlay must paint instantly and stay chrome-free, so there is no error boundary UI here: uncaught
	// errors are logged (and deduped) only.
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import { onMount, type Snippet } from "svelte";
	import { startErrorHandler } from "#lib/errors/errors.svelte.ts";

	let { children }: { children: Snippet } = $props();

	onMount(() => startErrorHandler("shots"));
</script>

{@render children()}
