<script lang="ts">
	import { Ban, Scale, Users, SlidersHorizontal, Cog, Play } from "@lucide/svelte";
	import type { RichView } from "#lib/viewers/fileType.ts";
	import {
		detectLicense,
		editorconfigDoc,
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
	import { m } from "#lib/i18n/m.ts";

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
					<span class="rv-ver"
						>{m.viewers_files_format_view_rules_count({
							count: ignore.reduce((n, s) => n + s.rules.length, 0),
						})}</span
					>
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
									title={r.negated
										? m.viewers_files_format_view_keep_tip()
										: m.viewers_files_format_view_ignore_tip()}
									>{r.negated
										? m.viewers_files_format_view_ignore_keep()
										: m.viewers_files_format_view_ignore_ignore()}</span
								>
								<span class="rv-k">{r.pattern}</span>
								{#if r.dirOnly}<span class="rv-chip">{m.viewers_files_format_view_directory()}</span>{/if}
								{#if r.anchored}<span class="rv-chip">{m.viewers_files_format_view_repo_root_only()}</span
									>{/if}
								<span class="rv-v"></span>
								<button class="rv-btn" onclick={() => ctx.toSource(r.line)}
									>{m.viewers_devops_line({ line: r.line })}</button
								>
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
						{m.viewers_files_format_view_copyright({ year: license.year, holder: license.holder })}
					</p>{/if}
				<div class="rv-facts">
					{#each license.summary as s (s)}<span class="rv-chip ok">{s}</span>{/each}
					{#if license.spdx}
						<button
							class="rv-chip acc link"
							onclick={() => api.openUrl(`https://spdx.org/licenses/${license.spdx}.html`)}
							>{m.viewers_files_format_view_spdx_page()}</button
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
					<span class="rv-ver"
						>{m.viewers_files_format_view_owners_summary({
							rules: owners.rules.length,
							owners: owners.owners.size,
						})}</span
					>
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
				<h2>
					{m.viewers_files_format_view_rules()}
					<span class="n">{m.viewers_files_format_view_last_match_wins()}</span>
				</h2>
				<div class="rv-list">
					{#each owners.rules as r (r.line)}
						<div class="rv-row">
							<span class="rv-k">{r.pattern}</span>
							<span class="rv-v"></span>
							{#each r.owners as o (o)}<span class="rv-chip mono">{o}</span>{/each}
							{#if !r.owners.length}<span class="rv-chip warn">{m.viewers_files_format_view_no_owner()}</span
								>{/if}
							<button class="rv-btn" onclick={() => ctx.toSource(r.line)}
								>{m.viewers_devops_line({ line: r.line })}</button
							>
						</div>
					{/each}
				</div>
			</section>
		{:else if kind === "editorconfig" && ec}
			<header class="rv-head">
				<div class="rv-title">
					<SlidersHorizontal size={18} />
					<h1>{name}</h1>
					<span class="rv-ver">{m.viewers_files_format_view_sections({ count: ec.sections.length })}</span>
					{#if ec.root}<span class="rv-chip acc">{m.viewers_files_format_view_root()}</span>{/if}
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
								<span class="rv-note">{editorconfigDoc(p.key)}</span>
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
					<span class="rv-ver">{m.viewers_files_format_view_processes({ count: procs.length })}</span>
				</div>
			</header>
			<div class="rv-list">
				{#each procs as p (p.line)}
					<div class="rv-row">
						<span class="rv-k">{p.name}</span>
						<span class="rv-v" title={p.command}>{p.command}</span>
						<button
							class="rv-btn run"
							title={m.viewers_files_package_view_run_in_terminal()}
							onclick={() => runInTerminal(ctx.worktreeId, ctx.path, ["sh", "-c", p.command], p.name)}
							><Play size={11} /> {m.viewers_devops_make_view_run()}</button
						>
						<Copy text={p.command} label={m.viewers_devops_copy_command()} />
					</div>
				{:else}
					<div class="rv-empty">{m.viewers_files_format_view_no_processes_defined()}</div>
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
