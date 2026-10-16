<script lang="ts">
	import { BookOpen, TriangleAlert } from "@lucide/svelte";
	import { parseData } from "#lib/viewers/rich/data.ts";
	import { readNotebook } from "#lib/viewers/rich/notebook.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Code from "#lib/viewers/rich/Code.svelte";
	import "#lib/viewers/rich/rich.css";

	let { ctx }: { ctx: RichCtx } = $props();

	let parsed = $derived(parseData(ctx.text, "json"));
	let nb = $derived(parsed.ok ? readNotebook(parsed.value) : null);
	const dir = $derived(ctx.path.split("/").slice(0, -1).join("/"));
	/** Notebook HTML outputs are untrusted: show them as text rather than injecting markup. */
	const strip = (h: string) =>
		h
			.replace(/<style[\s\S]*?<\/style>|<script[\s\S]*?<\/script>/gi, "")
			.replace(/<[^>]+>/g, "")
			.trim();
</script>

<div class="rv">
	<div class="rv-inner">
		{#if !parsed.ok}
			<div class="rv-banner bad">
				<TriangleAlert size={14} /> Cannot parse: {parsed.error}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource(parsed.line)}>Open as source</button>
			</div>
		{:else if nb}
			<header class="rv-head">
				<div class="rv-title">
					<BookOpen size={18} />
					<h1>{ctx.path.split("/").pop()}</h1>
					<span class="rv-ver">{nb.cells.length} cell{nb.cells.length === 1 ? "" : "s"}</span>
				</div>
				<div class="rv-facts">
					<span class="rv-chip info">{nb.kernel}</span>
					<span class="rv-chip">{nb.format}</span>
				</div>
			</header>
			<div class="cells">
				{#each nb.cells as c, i (i)}
					<section class="cell {c.type}">
						{#if c.type === "markdown"}
							<article class="md ca-selectable">
								<!-- eslint-disable-next-line svelte/no-at-html-tags -->{@html renderMarkdown(c.source, dir)}
							</article>
						{:else}
							<div class="in">
								<span class="ct">{c.type === "code" ? `[${c.execution ?? " "}]` : "raw"}</span>
								<div class="src"><Code code={c.source} lang={c.type === "code" ? nb.language : null} /></div>
							</div>
							{#each c.outputs as o, oi (oi)}
								<div class="out">
									<span class="ct"></span>
									<div class="body">
										{#if o.kind === "image"}
											<img src="data:{o.mime};base64,{o.data}" alt="Cell {i + 1} output" />
										{:else if o.kind === "html"}
											<pre class="rv-code ca-selectable">{strip(o.text)}</pre>
										{:else if o.kind === "error"}
											<pre class="rv-code ca-selectable err">{#if o.trace.includes(o.name)}{o.trace}{:else}<b
														>{o.name}</b
													>: {o.value}{o.trace ? "\n" + o.trace : ""}{/if}</pre>
										{:else}
											<pre class="rv-code ca-selectable" class:err={o.stream === "stderr"}>{o.text}</pre>
										{/if}
									</div>
								</div>
							{/each}
						{/if}
					</section>
				{/each}
			</div>
		{/if}
	</div>
</div>

<style>
	.cells {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.in,
	.out {
		display: flex;
		gap: 10px;
		min-width: 0;
	}
	.ct {
		flex: none;
		width: 44px;
		text-align: right;
		padding-top: 9px;
		font-family: var(--mono);
		font-size: 10.5px;
		color: var(--ca-text-secondary);
	}
	.src,
	.body {
		flex: 1;
		min-width: 0;
	}
	.src :global(pre) {
		border: 0.5px solid var(--rv-line);
		color: var(--cm-fg, var(--ca-text-primary));
	}
	.out {
		margin-top: 6px;
	}
	.out :global(pre) {
		background: none;
		padding-top: 4px;
		padding-bottom: 4px;
	}
	.err {
		color: var(--rv-bad);
	}
	img {
		max-width: 100%;
		border-radius: 6px;
		background: white;
	}
	.md {
		padding: 0 0 0 54px;
		line-height: 1.6;
		font-size: 13.5px;
	}
	.md :global(h1),
	.md :global(h2),
	.md :global(h3) {
		margin: 0.4em 0 0.3em;
		letter-spacing: -0.01em;
	}
	.md :global(p) {
		margin: 0.4em 0;
	}
	.md :global(code) {
		font-family: var(--mono);
		font-size: 0.9em;
		background: var(--rv-tint);
		padding: 0.1em 0.35em;
		border-radius: 4px;
	}
	.md :global(pre.code) {
		padding: 10px 12px;
		border-radius: 8px;
		background: var(--rv-tint);
		overflow: auto;
	}
	.md :global(pre.code code) {
		background: none;
		padding: 0;
	}
	.sp {
		flex: 1;
	}
	b {
		font-weight: 650;
	}
</style>
