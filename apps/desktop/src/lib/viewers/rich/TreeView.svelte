<script lang="ts">
	import { ChevronRight, Search, TriangleAlert, ChevronsDownUp, ChevronsUpDown } from "@lucide/svelte";
	import { fileType } from "#lib/viewers/fileType.ts";
	import { fileTypeLabel } from "#lib/viewers/fileTypeLabel.ts";
	import { formatOf, parseData } from "#lib/viewers/rich/data.ts";
	import { defaultOpen, flatten } from "#lib/viewers/rich/tree.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	const ROW_H = 24;
	const OVERSCAN = 14;

	let ft = $derived(fileType(ctx.path));
	let parsed = $derived(parseData(ctx.text, formatOf(ft.lang)));
	let open = $state(new Set<string>());
	let q = $state("");
	let top = $state(0);
	let height = $state(600);
	let seeded = "";

	// Seed the expansion once per file content shape (not on every keystroke in the source editor).
	$effect(() => {
		if (!parsed.ok) return;
		const key = ctx.path;
		if (seeded !== key) {
			seeded = key;
			open = defaultOpen(parsed.value);
		}
	});

	let flat = $derived(parsed.ok ? flatten(parsed.value, open, q) : { rows: [], matches: 0 });
	let first = $derived(Math.max(0, Math.floor(top / ROW_H) - OVERSCAN));
	let last = $derived(Math.min(flat.rows.length, Math.ceil((top + height) / ROW_H) + OVERSCAN));
	let visible = $derived(flat.rows.slice(first, last));
	let focus = $state(0);

	function toggle(id: string) {
		const n = new Set(open);
		if (n.has(id)) n.delete(id);
		else n.add(id);
		open = n;
	}

	function expandAll(on: boolean) {
		if (!parsed.ok) return;
		if (!on) return void (open = new Set(["$"]));
		const all = new Set<string>();
		const walk = (id: string) => {
			all.add(id);
		};
		for (const r of flatten(parsed.value, new Set(), "").rows) walk(r.id);
		// `flatten` only lists expanded rows, so grow until stable (bounded by the file size cap).
		let size = -1;
		while (all.size !== size) {
			size = all.size;
			for (const r of flatten(parsed.value, all, "").rows) all.add(r.id);
		}
		open = all;
	}

	function onkey(e: KeyboardEvent) {
		const r = flat.rows[focus];
		if (!r) return;
		if (e.key === "ArrowDown") focus = Math.min(flat.rows.length - 1, focus + 1);
		else if (e.key === "ArrowUp") focus = Math.max(0, focus - 1);
		else if (e.key === "ArrowRight" && r.expandable && !r.expanded) toggle(r.id);
		else if (e.key === "ArrowLeft" && r.expanded) toggle(r.id);
		else if ((e.key === "Enter" || e.key === " ") && r.expandable) toggle(r.id);
		else return;
		e.preventDefault();
		const el = e.currentTarget as HTMLElement;
		if (focus * ROW_H < el.scrollTop) el.scrollTop = focus * ROW_H;
		else if ((focus + 1) * ROW_H > el.scrollTop + height) el.scrollTop = (focus + 1) * ROW_H - height;
	}
</script>

<div class="rv tv">
	{#if !parsed.ok}
		<div class="rv-inner">
			<div class="rv-banner bad">
				<TriangleAlert size={14} />
				{m.viewers_files_tree_view_cannot_parse({ format: fileTypeLabel(ft), error: parsed.error })}
				<span class="sp"></span>
				<button class="rv-btn" onclick={() => ctx.toSource(parsed.line)}>
					{m.viewers_devops_open_as_source()}
				</button>
			</div>
		</div>
	{:else}
		<div class="bar">
			<span class="rv-ver">
				{m.viewers_files_tree_view_visible({ format: fileTypeLabel(ft), count: flat.rows.length })}
			</span>
			{#if q}<span class="rv-chip acc">
					{m.viewers_files_tree_view_matches({ count: flat.matches })}
				</span>{/if}
			<span class="sp"></span>
			<label class="rv-search">
				<Search size={12} />
				<input
					placeholder={m.viewers_files_tree_view_search_keys_and_values()}
					bind:value={q}
					aria-label={m.common_action_search()}
				/>
			</label>
			<button
				class="rv-btn icon"
				title={m.viewers_files_tree_view_expand_all()}
				aria-label={m.viewers_files_tree_view_expand_all()}
				onclick={() => expandAll(true)}
			>
				<ChevronsUpDown size={13} />
			</button>
			<button
				class="rv-btn icon"
				title={m.viewers_files_tree_view_collapse_all()}
				aria-label={m.viewers_files_tree_view_collapse_all()}
				onclick={() => expandAll(false)}
			>
				<ChevronsDownUp size={13} />
			</button>
		</div>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="scroll"
			bind:clientHeight={height}
			onscroll={(e) => (top = e.currentTarget.scrollTop)}
			onkeydown={onkey}
			role="tree"
			tabindex="0"
		>
			<div style:height="{flat.rows.length * ROW_H}px">
				<div style:transform="translateY({first * ROW_H}px)">
					{#each visible as r, i (r.id)}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<div
							class="n"
							class:hit={r.hit}
							class:fcs={focus === first + i}
							style:padding-left="{10 + r.depth * 16}px"
							role="treeitem"
							aria-level={r.depth + 1}
							aria-selected={focus === first + i}
							aria-expanded={r.expandable ? r.expanded : undefined}
							tabindex="-1"
							onclick={() => {
								focus = first + i;
								if (r.expandable) toggle(r.id);
							}}
						>
							<span class="tw" class:rot={r.expanded} class:none={!r.expandable}>
								<ChevronRight size={12} />
							</span>
							<span class="k">{r.key}</span>
							<span class="v t-{r.type}" class:c={r.expandable}>
								{r.type === "string" ? JSON.stringify(r.preview) : r.preview}
							</span>
							<span class="ty">{r.type}</span>
							<span class="act">
								{#if r.path}<Copy text={r.path} label={m.viewers_files_tree_view_copy_path()} />{/if}
							</span>
						</div>
					{/each}
				</div>
			</div>
			{#if !flat.rows.length}<div class="rv-empty">{m.viewers_files_tree_view_nothing_matches()}</div>{/if}
		</div>
	{/if}
</div>

<style>
	.tv {
		overflow: hidden;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		border-bottom: 0.5px solid var(--rv-line);
		color: var(--ca-text-secondary);
	}
	.sp {
		flex: 1;
	}
	.scroll {
		flex: 1;
		min-height: 0;
		overflow: auto;
		outline: none;
		padding: 6px 0;
	}
	.n {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 24px;
		padding-right: 8px;
		font-family: var(--mono);
		font-size: 12px;
		cursor: default;
		min-width: 0;
	}
	.n:hover {
		background: var(--rv-tint);
	}
	.n.fcs {
		background: color-mix(in oklab, var(--rv-acc) 9%, transparent);
	}
	.n.hit .k {
		background: color-mix(in oklab, var(--rv-warn) 30%, transparent);
		border-radius: 3px;
	}
	.tw {
		display: inline-flex;
		width: 14px;
		flex: none;
		color: var(--ca-text-disabled);
		transition: transform var(--dur-fast, 120ms) var(--ease, ease);
	}
	.tw.rot {
		transform: rotate(90deg);
	}
	.tw.none {
		visibility: hidden;
	}
	.k {
		font-weight: 550;
		flex: none;
		color: var(--cm-syn-property, var(--cm-fg, var(--ca-text-primary)));
	}
	.v {
		min-width: 0;
		flex: 0 1 auto;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.v.t-string {
		color: var(--cm-syn-string, var(--rv-ok));
	}
	.v.t-number,
	.v.t-date {
		color: var(--cm-syn-number, var(--rv-warn));
	}
	.v.t-boolean {
		color: var(--cm-syn-boolean, var(--rv-warn));
	}
	.v.t-null {
		color: var(--cm-syn-constant, var(--rv-warn));
	}
	.v.c {
		color: var(--ca-text-secondary);
	}
	.ty {
		margin-left: auto;
		flex: none;
		font-size: 10px;
		color: var(--ca-text-secondary);
		opacity: 0;
	}
	.n:hover .ty,
	.n.fcs .ty {
		opacity: 1;
	}
	.act {
		flex: none;
		width: 22px;
		opacity: 0;
	}
	.n:hover .act,
	.n.fcs .act {
		opacity: 1;
	}
</style>
