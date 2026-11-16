<script lang="ts">
	/** A link that opens in the system browser (a Tauri webview must not navigate away from Atlas). */
	import type { Snippet } from "svelte";
	import { toast } from "@codeatlasdev/ui";
	import { api, errText } from "#lib/api.ts";

	let { href, children }: { href: string; children: Snippet } = $props();
</script>

<a
	{href}
	onclick={(e) => {
		e.preventDefault();
		api.openUrl(href).catch((err) => toast.error(errText(err)));
	}}>{@render children()}</a
>

<style>
	a {
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	a:hover {
		text-decoration: underline;
	}
</style>
