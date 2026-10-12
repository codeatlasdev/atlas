<script lang="ts">
	// The menu-bar popover is its own webview: no app shell, no workspace, no menu sync.
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import type { Snippet } from "svelte";

	let { children }: { children: Snippet } = $props();
</script>

{@render children()}
