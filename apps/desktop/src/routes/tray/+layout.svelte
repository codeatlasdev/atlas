<script lang="ts">
	// The menu-bar popover is its own webview: no app shell, no workspace, no menu sync.
	import "@codeatlasdev/ui/styles/global.css";
	import "@codeatlasdev/ui/styles/materials.css";
	import "../../app.css";
	import { onMount, type Snippet } from "svelte";
	import Contain from "#lib/errors/Contain.svelte";
	import { startErrorHandler } from "#lib/errors/errors.svelte.ts";
	import { IS_MAC, appLauncher } from "#lib/keys.ts";

	let { children }: { children: Snippet } = $props();

	// No toast stack in a popover this small: uncaught errors are logged (and deduped) only.
	onMount(() => startErrorHandler("tray"));
</script>

<Contain
	where={IS_MAC ? "Menu bar popover" : "Tray popover"}
	title="The popover hit an error"
	hint="Open Atlas from {appLauncher()} to keep working."
>
	{@render children()}
</Contain>
