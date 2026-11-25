<script lang="ts">
	import { formatBytes } from "#lib/viewers/kind.ts";
	import type { RichView } from "#lib/viewers/fileType.ts";
	import { RICH_MAX_BYTES, type RichCtx } from "#lib/viewers/rich/ctx.ts";
	import EnvView from "#lib/viewers/rich/EnvView.svelte";
	import PackageView from "#lib/viewers/rich/PackageView.svelte";
	import DockerfileView from "#lib/viewers/rich/DockerfileView.svelte";
	import ComposeView from "#lib/viewers/rich/ComposeView.svelte";
	import MakeView from "#lib/viewers/rich/MakeView.svelte";
	import AtlasView from "#lib/viewers/rich/AtlasView.svelte";
	import TableView from "#lib/viewers/rich/TableView.svelte";
	import TreeView from "#lib/viewers/rich/TreeView.svelte";
	import WorkflowView from "#lib/viewers/rich/WorkflowView.svelte";
	import TsconfigView from "#lib/viewers/rich/TsconfigView.svelte";
	import FormatView from "#lib/viewers/rich/FormatView.svelte";
	import NotebookView from "#lib/viewers/rich/NotebookView.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	/** Picks the rich view for a file type; huge files ask before rendering. */
	let { view, ctx }: { view: RichView; ctx: RichCtx } = $props();
	let force = $state(false);
	let big = $derived(ctx.size > RICH_MAX_BYTES && !force);
</script>

{#if big}
	<div class="rv">
		<div class="rv-inner">
			<div class="rv-banner warn">
				{m.viewers_files_rich_view_too_large({ size: formatBytes(ctx.size) })}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource()}>{m.viewers_devops_open_as_source()}</button>
				<button class="rv-btn" onclick={() => (force = true)}>
					{m.viewers_files_rich_view_render_anyway()}
				</button>
			</div>
		</div>
	</div>
{:else if view === "env"}
	<EnvView {ctx} />
{:else if view === "package-json" || view === "manifest"}
	<PackageView {ctx} />
{:else if view === "dockerfile"}
	<DockerfileView {ctx} />
{:else if view === "compose"}
	<ComposeView {ctx} />
{:else if view === "makefile" || view === "justfile"}
	<MakeView {ctx} just={view === "justfile"} />
{:else if view === "atlas"}
	<AtlasView {ctx} />
{:else if view === "table"}
	<TableView {ctx} />
{:else if view === "tree"}
	<TreeView {ctx} />
{:else if view === "workflow"}
	<WorkflowView {ctx} />
{:else if view === "tsconfig"}
	<TsconfigView {ctx} />
{:else if view === "notebook"}
	<NotebookView {ctx} />
{:else}
	<FormatView {ctx} kind={view} />
{/if}

<style>
	.sp {
		flex: 1;
	}
</style>
