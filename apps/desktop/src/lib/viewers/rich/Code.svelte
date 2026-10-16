<script lang="ts">
	import { highlightLang } from "#lib/viewers/rich/highlight.ts";

	/** Highlighted snippet (the grammar loads lazily); shows plain text until it is ready. */
	let { code, lang, inline = false }: { code: string; lang: string | null; inline?: boolean } = $props();

	const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
	let html = $state("");
	let shown = $derived(html || esc(code));

	$effect(() => {
		const c = code;
		let live = true;
		void highlightLang(c, lang).then((h) => {
			if (live) html = h;
		});
		return () => {
			live = false;
			html = "";
		};
	});
</script>

{#if inline}
	<code class="rv-ic ca-selectable"
		><!-- eslint-disable-next-line svelte/no-at-html-tags -->{@html shown}</code
	>
{:else}
	<pre
		class="rv-code ca-selectable"><!-- eslint-disable-next-line svelte/no-at-html-tags -->{@html shown}</pre>
{/if}

<style>
	.rv-ic {
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--cm-fg, var(--ca-text-primary));
	}
	pre {
		color: var(--cm-fg, var(--ca-text-primary));
	}
</style>
