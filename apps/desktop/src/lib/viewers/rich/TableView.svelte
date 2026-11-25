<script lang="ts">
	import { ArrowUp, ArrowDown, Search, Table2, TriangleAlert } from "@lucide/svelte";
	import { compareCells, readTable } from "#lib/viewers/rich/csv.ts";
	import type { RichCtx } from "#lib/viewers/rich/ctx.ts";
	import Copy from "#lib/viewers/rich/Copy.svelte";
	import "#lib/viewers/rich/rich.css";
	import { m } from "#lib/i18n/m.ts";

	let { ctx }: { ctx: RichCtx } = $props();

	const ROW_H = 28;
	const OVERSCAN = 12;

	let table = $derived(readTable(ctx.text, /\.tsv$/i.test(ctx.path) ? "\t" : undefined));
	let sort = $state<{ col: number; dir: 1 | -1 } | null>(null);
	let q = $state("");
	let scroller: HTMLDivElement | undefined = $state();
	let top = $state(0);
	let height = $state(600);
	let sel = $state<{ r: number; c: number } | null>(null);

	let cols = $derived(table.header?.length ?? Math.max(0, ...table.rows.slice(0, 200).map((r) => r.length)));
	let head = $derived(
		table.header ??
			Array.from(
				{ length: cols },
				(_, i) => String.fromCharCode(65 + (i % 26)) + (i >= 26 ? Math.floor(i / 26) : ""),
			),
	);
	let numeric = $derived(
		head.map((_, c) => {
			const sample = table.rows
				.slice(0, 100)
				.map((r) => r[c] ?? "")
				.filter((x) => x !== "");
			return sample.length > 0 && sample.every((x) => !Number.isNaN(Number(x)));
		}),
	);
	let rows = $derived.by(() => {
		const s = q.trim().toLowerCase();
		let out = s ? table.rows.filter((r) => r.some((c) => c.toLowerCase().includes(s))) : table.rows;
		if (sort) {
			const { col, dir } = sort;
			out = [...out].sort((a, b) => dir * compareCells(a[col] ?? "", b[col] ?? ""));
		}
		return out;
	});
	let first = $derived(Math.max(0, Math.floor(top / ROW_H) - OVERSCAN));
	let last = $derived(Math.min(rows.length, Math.ceil((top + height) / ROW_H) + OVERSCAN));
	let visible = $derived(rows.slice(first, last));
	/** Column widths from the header and a sample of rows, clamped. */
	let widths = $derived(
		head.map((h, c) => {
			const longest = Math.max(h.length, ...table.rows.slice(0, 60).map((r) => (r[c] ?? "").length));
			return Math.min(Math.max(longest * 7.4 + 24, 72), 320);
		}),
	);
	let total = $derived(widths.reduce((a, b) => a + b, 52));

	function toggle(c: number) {
		sort = sort?.col !== c ? { col: c, dir: 1 } : sort.dir === 1 ? { col: c, dir: -1 } : null;
		if (scroller) scroller.scrollTop = 0;
	}

	function onkey(e: KeyboardEvent) {
		if (!sel) return;
		const d = { ArrowDown: [1, 0], ArrowUp: [-1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[e.key];
		if (!d) return;
		e.preventDefault();
		const r = Math.min(rows.length - 1, Math.max(0, sel.r + d[0]));
		sel = { r, c: Math.min(cols - 1, Math.max(0, sel.c + d[1])) };
		if (scroller) {
			if (r * ROW_H < scroller.scrollTop) scroller.scrollTop = r * ROW_H;
			else if ((r + 1) * ROW_H > scroller.scrollTop + height - ROW_H)
				scroller.scrollTop = (r + 2) * ROW_H - height;
		}
	}
</script>

<div class="rv tv">
	<div class="bar">
		<Table2 size={14} />
		<span class="rv-ver">
			{q
				? m.viewers_files_table_view_rows_filtered({ count: rows.length, total: table.rows.length })
				: m.viewers_files_table_view_rows({ count: rows.length })} · {m.viewers_files_table_view_columns({
				count: cols,
			})}
		</span>
		{#if !table.header}<span class="rv-chip">{m.viewers_files_table_view_no_header()}</span>{/if}
		{#if table.ragged}<span class="rv-chip warn">
				<TriangleAlert size={10} />
				{m.viewers_files_table_view_ragged({ count: table.ragged })}
			</span>{/if}
		<span class="rv-chip mono">
			{table.delimiter === "\t"
				? m.viewers_files_table_view_delimiter_tab()
				: table.delimiter === ","
					? m.viewers_files_table_view_delimiter_comma()
					: table.delimiter === ";"
						? m.viewers_files_table_view_delimiter_semicolon()
						: m.viewers_files_table_view_delimiter_pipe()}
		</span>
		<span class="sp"></span>
		<label class="rv-search">
			<Search size={12} />
			<input
				placeholder={m.viewers_files_table_view_filter_rows()}
				bind:value={q}
				aria-label={m.viewers_files_table_view_filter_rows()}
			/>
		</label>
	</div>
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div
		class="scroll"
		bind:this={scroller}
		bind:clientHeight={height}
		onscroll={(e) => (top = e.currentTarget.scrollTop)}
		onkeydown={onkey}
		role="grid"
		aria-rowcount={rows.length + 1}
		aria-colcount={cols}
		tabindex="0"
	>
		<div class="grid" style:width="{total}px">
			<div class="hd" role="row">
				<span class="ix"></span>
				{#each head as h, c (c)}
					<button
						class="th"
						class:num={numeric[c]}
						style:width="{widths[c]}px"
						onclick={() => toggle(c)}
						title={m.viewers_files_table_view_sort_by({ column: h })}
					>
						<span class="t">{h}</span>
						{#if sort?.col === c}{#if sort.dir === 1}<ArrowUp size={11} />{:else}<ArrowDown
									size={11}
								/>{/if}{/if}
					</button>
				{/each}
			</div>
			<div style:height="{rows.length * ROW_H}px" class="body">
				<div style:transform="translateY({first * ROW_H}px)">
					{#each visible as row, i (first + i)}
						{@const r = first + i}
						<div class="tr" class:odd={r % 2} role="row" aria-rowindex={r + 2}>
							<span class="ix">{r + 1}</span>
							{#each head as _, c (c)}
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<span
									class="td"
									class:num={numeric[c]}
									class:sel={sel?.r === r && sel?.c === c}
									style:width="{widths[c]}px"
									role="gridcell"
									tabindex="-1"
									title={row[c] ?? ""}
									onclick={() => (sel = { r, c })}
								>
									{row[c] ?? ""}
								</span>
							{/each}
						</div>
					{/each}
				</div>
			</div>
		</div>
		{#if !rows.length}<div class="rv-empty">
				{q ? m.viewers_files_table_view_no_rows_match() : m.viewers_files_table_view_empty_file()}
			</div>{/if}
	</div>
	{#if sel && rows[sel.r]}
		<div class="foot">
			<span class="rv-chip mono">
				{m.viewers_files_table_view_cell({ row: sel.r + 1, column: head[sel.c] })}
			</span>
			<span class="val">{rows[sel.r][sel.c] ?? ""}</span>
			<Copy text={rows[sel.r][sel.c] ?? ""} label={m.viewers_files_table_view_copy_cell()} />
			<Copy text={rows[sel.r].join(table.delimiter)} label={m.viewers_files_table_view_copy_row()} />
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
		position: relative;
	}
	.grid {
		min-width: 100%;
	}
	.hd {
		position: sticky;
		top: 0;
		z-index: 2;
		display: flex;
		height: 28px;
		background: color-mix(
			in oklab,
			var(--ca-surface-raised, var(--ca-bg-default)) 92%,
			var(--ca-text-primary) 8%
		);
		border-bottom: 0.5px solid var(--rv-line);
	}
	.ix {
		flex: none;
		width: 52px;
		padding: 0 8px;
		text-align: right;
		line-height: 28px;
		font-family: var(--mono);
		font-size: 10.5px;
		color: var(--ca-text-disabled);
		position: sticky;
		left: 0;
		background: inherit;
		user-select: none;
	}
	.th {
		flex: none;
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 10px;
		border: 0;
		border-right: 0.5px solid var(--rv-line);
		background: none;
		font: inherit;
		font-size: 11.5px;
		font-weight: 650;
		color: var(--ca-text-primary);
		cursor: pointer;
		text-align: left;
		overflow: hidden;
	}
	.th:hover {
		background: var(--ca-action-hover);
	}
	.th.num {
		justify-content: flex-end;
	}
	.t {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tr {
		display: flex;
		height: 28px;
	}
	.tr.odd {
		background: color-mix(in oklab, var(--ca-text-primary) 2.5%, transparent);
	}
	.tr:hover {
		background: var(--rv-tint);
	}
	.tr .ix {
		background: var(--ca-surface-raised, var(--ca-bg-default));
	}
	.td {
		flex: none;
		padding: 0 10px;
		line-height: 28px;
		font-family: var(--mono);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		border-right: 0.5px solid color-mix(in oklab, var(--rv-line) 60%, transparent);
	}
	.td.num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.td.sel {
		box-shadow: inset 0 0 0 1.5px var(--rv-acc);
		background: color-mix(in oklab, var(--rv-acc) 8%, transparent);
	}
	.foot {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		border-top: 0.5px solid var(--rv-line);
	}
	.val {
		flex: 1;
		min-width: 0;
		font-family: var(--mono);
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
