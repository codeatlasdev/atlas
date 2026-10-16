<script lang="ts">
	import { onMount } from "svelte";
	import { host, b64bytes } from "#lib/host.ts";
	import { MIME, extOf } from "#lib/viewers/kind.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";

	let { text, path, root }: { text: string; path: string; root: string } = $props();

	let el: HTMLElement | undefined = $state();
	let html = $derived(renderMarkdown(text, path.split("/").slice(0, -1).join("/")));
	const urls = new Map<string, string>();

	// Rendered from the user's own file; marked output is not sanitized, but CSP blocks inline script.
	$effect(() => {
		void html;
		if (!el) return;
		for (const img of el.querySelectorAll<HTMLImageElement>("img[data-src]")) {
			const rel = img.dataset.src!;
			const hit = urls.get(rel);
			if (hit) {
				img.src = hit;
				continue;
			}
			void host
				.fsRead(`${root}/${rel}`, 16 * 1024 * 1024)
				.then((r) => {
					const type = MIME[extOf(rel)] ?? "application/octet-stream";
					const u = URL.createObjectURL(new Blob([b64bytes(r.data) as Uint8Array<ArrayBuffer>], { type }));
					urls.set(rel, u);
					img.src = u;
				})
				.catch(() => img.classList.add("missing"));
		}
	});

	onMount(() => () => {
		for (const u of urls.values()) URL.revokeObjectURL(u);
	});
</script>

<article class="md ca-selectable" bind:this={el}>
	<!-- eslint-disable-next-line svelte/no-at-html-tags -->
	{@html html}
</article>

<style>
	.md {
		flex: 1;
		overflow: auto;
		padding: 28px max(28px, calc((100% - 760px) / 2)) 64px;
		font-size: 14px;
		line-height: 1.7;
		color: var(--ca-text-primary);
		text-wrap: pretty;
	}
	.md :global(> :first-child) {
		margin-top: 0;
	}
	.md :global(h1),
	.md :global(h2),
	.md :global(h3),
	.md :global(h4) {
		color: var(--cm-syn-title, var(--ca-text-primary));
		line-height: 1.25;
		letter-spacing: -0.015em;
		margin: 1.8em 0 0.6em;
		font-weight: 700;
	}
	.md :global(h1) {
		font-size: 1.9em;
		padding-bottom: 0.3em;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.md :global(h2) {
		font-size: 1.4em;
		padding-bottom: 0.25em;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.md :global(h3) {
		font-size: 1.15em;
	}
	.md :global(h4) {
		font-size: 1em;
		color: var(--cm-syn-title, var(--ca-text-secondary));
	}
	.md :global(p),
	.md :global(ul),
	.md :global(ol),
	.md :global(blockquote),
	.md :global(table),
	.md :global(pre) {
		margin: 0 0 1em;
	}
	.md :global(a) {
		color: var(--cm-syn-link-uri, var(--space-accent, var(--ca-brand-main)));
		text-decoration: none;
		border-bottom: 1px solid color-mix(in oklab, currentColor 35%, transparent);
	}
	.md :global(a:hover) {
		color: var(--atlas-link, var(--cm-syn-link-uri, var(--ca-brand-main)));
		border-bottom-color: currentColor;
	}
	.md :global(code) {
		color: var(--cm-syn-text-literal, inherit);
		font-family: var(--mono);
		font-size: 0.88em;
		padding: 0.15em 0.4em;
		border-radius: 5px;
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.md :global(pre.code) {
		position: relative;
		padding: 14px 16px;
		overflow: auto;
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 4.5%, transparent);
		line-height: 1.55;
	}
	.md :global(pre.code code) {
		padding: 0;
		background: none;
		font-size: 12.5px;
		color: var(--cm-fg, inherit);
	}
	.md :global(pre.code code[data-lang])::before {
		content: attr(data-lang);
		position: absolute;
		top: 6px;
		right: 10px;
		font-size: 10.5px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-text-disabled);
	}
	.md :global(blockquote) {
		padding: 2px 16px;
		border-left: 3px solid var(--ca-outline-default);
		color: var(--ca-text-secondary);
	}
	.md :global(table) {
		border-collapse: separate;
		border-spacing: 0;
		display: block;
		overflow-x: auto;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
	}
	.md :global(th),
	.md :global(td) {
		padding: 7px 14px;
		text-align: left;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.md :global(tr:last-child td) {
		border-bottom: 0;
	}
	.md :global(th) {
		font-weight: 650;
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
	}
	.md :global(img) {
		max-width: 100%;
		border-radius: 8px;
	}
	.md :global(img.missing) {
		min-height: 24px;
		outline: 1px dashed var(--ca-outline-subtle);
	}
	.md :global(hr) {
		border: 0;
		border-top: 0.5px solid var(--ca-outline-subtle);
		margin: 2em 0;
	}
	.md :global(ul:has(input[type="checkbox"])) {
		list-style: none;
		padding-left: 4px;
	}
	.md :global(li > input[type="checkbox"]) {
		margin-right: 8px;
		accent-color: var(--space-accent, var(--ca-brand-main));
	}
	.md :global(li) {
		margin: 0.2em 0;
	}
</style>
