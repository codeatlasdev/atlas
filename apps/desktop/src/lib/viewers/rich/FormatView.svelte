<script lang="ts">
	import { Ban, Scale, Users, SlidersHorizontal, Cog, Play } from "@lucide/svelte";
	import type { RichView } from "#lib/viewers/fileType.ts";
	import {
		detectLicense,
		EDITORCONFIG_DOCS,
		ignoreSections,
		parseCodeowners,
		parseEditorconfig,
		parseProcfile,
	} from "#lib/viewers/rich/misc.ts";
	import { runInTerminal } from "#lib/viewers/rich/run.ts";
	import { api } from "#lib/api.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";

	/** One component for the small line-oriented formats: each is a short list, none deserves its own file. */
	let { ctx, kind }: { ctx: RichCtx; kind: RichView } = $props();

	let ignore = $derived(kind === "ignore" ? ignoreSections(ctx.text) : []);
	let license = $derived(kind === "license" ? detectLicense(ctx.text) : null);
	let owners = $derived(kind === "codeowners" ? parseCodeowners(ctx.text) : null);
	let ec = $derived(kind === "editorconfig" ? parseEditorconfig(ctx.text) : null);
	let procs = $derived(kind === "procfile" ? parseProcfile(ctx.text) : []);
	const name = $derived(ctx.path.split("/").pop());
	const ownerUrl = (o: string) =>
		o.startsWith("@") ? `https://github.com/${o.slice(1).split("/")[0]}` : null;
</script>

<div class="rv">
	<div class="rv-inner">
		{#if kind === "ignore"}
			<header class="rv-head">
				<div class="rv-title">
					<Ban size={18} />
					<h1>{name}</h1>
					<span class="rv-ver">{ignore.reduce((n, s) => n + s.rules.length, 0)} rules</span>
				</div>
			</header>
			{#each ignore as s, i (i)}
				<section class="rv-sec">
					{#if s.title}<h2>{s.title} <span class="n">{s.rules.length}</span></h2>{/if}
					<div class="rv-list">
						{#each s.rules as r (r.line)}
							<div class="rv-row">
								<span
									class="rv-chip"
									class:ok={r.negated}
									title={r.negated ? "Re-includes matches" : "Ignores matches"}
									>{r.negated ? "keep" : "ignore"}</span
								>
								<span class="rv-k">{r.pattern}</span>
								{#if r.dirOnly}<span class="rv-chip">directory</span>{/if}
								{#if r.anchored}<span class="rv-chip">repo root only</span>{/if}
								<span class="rv-v"></span>
								<button class="rv-btn" onclick={() => ctx.toSource(r.line)}>Line {r.line}</button>
							</div>
						{/each}
					</div>
				</section>
			{/each}
		{:else if kind === "license" && license}
			<header class="rv-head">
				<div class="rv-title">
					<Scale size={18} />
					<h1>{license.name}</h1>
					{#if license.spdx}<span class="rv-chip acc mono">{license.spdx}</span>{/if}
				</div>
				{#if license.holder || license.year}<p class="rv-desc">
						Copyright {license.year}
						{license.holder}
					</p>{/if}
				<div class="rv-facts">
					{#each license.summary as s (s)}<span class="rv-chip ok">{s}</span>{/each}
					{#if license.spdx}
						<button
							class="rv-chip acc link"
							onclick={() => api.openUrl(`https://spdx.org/licenses/${license.spdx}.html`)}>SPDX page</button
						>
					{/if}
				</div>
			</header>
			<pre class="rv-code ca-selectable text">{ctx.text}</pre>
		{:else if kind === "codeowners" && owners}
			<header class="rv-head">
				<div class="rv-title">
					<Users size={18} />
					<h1>{name}</h1>
					<span class="rv-ver">{owners.rules.length} rules · {owners.owners.size} owners</span>
				</div>
				<div class="rv-facts">
					{#each owners.owners as [o, n] (o)}
						{@const u = ownerUrl(o)}
						{#if u}<button class="rv-chip acc link" onclick={() => api.openUrl(u)}
								>{o} <span class="rv-note">{n}</span></button
							>
						{:else}<span class="rv-chip">{o} <span class="rv-note">{n}</span></span>{/if}
					{/each}
				</div>
			</header>
			<section class="rv-sec">
				<h2>Rules <span class="n">last match wins</span></h2>
				<div class="rv-list">
					{#each owners.rules as r (r.line)}
						<div class="rv-row">
							<span class="rv-k">{r.pattern}</span>
							<span class="rv-v"></span>
							{#each r.owners as o (o)}<span class="rv-chip mono">{o}</span>{/each}
							{#if !r.owners.length}<span class="rv-chip warn">no owner</span>{/if}
							<button class="rv-btn" onclick={() => ctx.toSource(r.line)}>Line {r.line}</button>
						</div>
					{/each}
				</div>
			</section>
		{:else if kind === "editorconfig" && ec}
			<header class="rv-head">
				<div class="rv-title">
					<SlidersHorizontal size={18} />
					<h1>{name}</h1>
					<span class="rv-ver">{ec.sections.length} section{ec.sections.length === 1 ? "" : "s"}</span>
					{#if ec.root}<span class="rv-chip acc">root</span>{/if}
				</div>
			</header>
			{#each ec.sections as s (s.glob)}
				<section class="rv-sec">
					<h2><span class="rv-k">{s.glob}</span></h2>
					<div class="rv-list">
						{#each s.props as p (p.line)}
							<div class="rv-row">
								<span class="rv-k">{p.key}</span>
								<span class="rv-chip mono" class:ok={p.value === "true"} class:bad={p.value === "false"}
									>{p.value}</span
								>
								<span class="rv-note">{EDITORCONFIG_DOCS[p.key] ?? ""}</span>
							</div>
						{/each}
					</div>
				</section>
			{/each}
		{:else if kind === "procfile"}
			<header class="rv-head">
				<div class="rv-title">
					<Cog size={18} />
					<h1>{name}</h1>
					<span class="rv-ver">{procs.length} process{procs.length === 1 ? "" : "es"}</span>
				</div>
			</header>
			<div class="rv-list">
				{#each procs as p (p.line)}
					<div class="rv-row">
						<span class="rv-k">{p.name}</span>
						<span class="rv-v" title={p.command}>{p.command}</span>
						<button
							class="rv-btn run"
							title="Run in a terminal"
							onclick={() => runInTerminal(ctx.worktreeId, ctx.path, ["sh", "-c", p.command], p.name)}
							><Play size={11} /> Run</button
						>
						<Copy text={p.command} label="Copy command" />
					</div>
				{:else}
					<div class="rv-empty">No processes defined</div>
				{/each}
			</div>
		{/if}
	</div>
</div>

<style>
	.link {
		border: 0;
		cursor: pointer;
		font: inherit;
		font-size: 11px;
	}
	.text {
		max-height: none;
		font-size: 12px;
	}
</style>
