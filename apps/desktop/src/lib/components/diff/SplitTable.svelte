<script lang="ts">
	/**
	 * One hunk side by side: old on the left, new on the right, deleted/added lines paired into rows.
	 * Both halves are cells of the same table row, so they scroll together by construction.
	 */
	import { MessageSquarePlus } from "@lucide/svelte";
	import type { DiffHunk } from "#lib/api.ts";
	import type { DiffComment } from "#lib/stores/workspace.svelte.ts";
	import DiffNotes from "./DiffNotes.svelte";
	import type { Draft } from "./DiffTable.svelte";
	import { changedSpan, splitRows, type Cell } from "./splitRows.ts";

	let {
		path,
		hunk,
		tokens,
		comments,
		draft = $bindable(),
		onsave,
		onresolve,
	}: {
		path: string;
		hunk: DiffHunk;
		/** Highlighted HTML per line of this hunk, once the grammar resolved. */
		tokens: string[] | undefined;
		comments: DiffComment[];
		draft: Draft | null;
		onsave: () => void;
		onresolve: (id: string, resolved: boolean) => void;
	} = $props();

	let rows = $derived(splitRows(hunk.lines));

	const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

	/** Plain text of a changed cell with the differing characters marked; syntax-highlighted lines keep their tokens. */
	function inline(cell: Cell, other: Cell | null, old: boolean): string | undefined {
		if (tokens?.[cell.idx] !== undefined) return tokens[cell.idx];
		const span = other && changedSpan(old ? cell.text : other.text, old ? other.text : cell.text);
		if (!span) return undefined;
		const to = old ? span.oldTo : span.newTo;
		if (to === span.from) return undefined;
		return `${esc(cell.text.slice(0, span.from))}<mark>${esc(cell.text.slice(span.from, to))}</mark>${esc(cell.text.slice(to))}`;
	}
</script>

<table>
	<colgroup>
		<col class="no" /><col class="gut" /><col />
		<col class="no" /><col class="gut" /><col />
	</colgroup>
	<tbody>
		{#each rows as r, ri (ri)}
			<tr class={r.kind}>
				{#each [{ cell: r.left, other: r.right, old: true }, { cell: r.right, other: r.left, old: false }] as s (s.old)}
					{@const side = s.old ? "old" : "new"}
					{@const cls = r.kind === "ctx" ? "ctx" : s.cell ? (s.old ? "del" : "add") : "empty"}
					{@const html =
						s.cell && r.kind === "change" ? inline(s.cell, s.other, s.old) : s.cell && tokens?.[s.cell.idx]}
					<td class="no {cls}" class:first={!s.old}>{s.cell?.no ?? ""}</td>
					<td class="gut {cls}">
						{#if s.cell?.no !== undefined}
							{@const no = s.cell.no}
							<button
								class="add-note"
								title="Comment (c)"
								onclick={() => (draft = { path, line: no, side, text: "" })}
							>
								<MessageSquarePlus size={11} />
							</button>
						{/if}
					</td>
					<td class="code mono {cls}">
						{#if s.cell}{#if html}{@html html}{:else}{s.cell.text}{/if}{/if}
					</td>
				{/each}
			</tr>
			<DiffNotes {path} side="old" line={r.left?.no} cols={6} {comments} bind:draft {onsave} {onresolve} />
			<DiffNotes {path} side="new" line={r.right?.no} cols={6} {comments} bind:draft {onsave} {onresolve} />
		{/each}
	</tbody>
</table>

<style>
	table {
		width: 100%;
		border-collapse: collapse;
		table-layout: fixed;
	}
	col.no {
		width: 44px;
	}
	col.gut {
		width: 18px;
	}
	td.no {
		text-align: right;
		padding: 0 6px;
		color: var(--ca-text-disabled);
		font-variant-numeric: tabular-nums;
		user-select: none;
	}
	td.first {
		border-left: 0.5px solid var(--ca-outline-subtle);
	}
	.add-note {
		opacity: 0;
		border: 0;
		background: none;
		padding: 0;
		color: var(--ca-brand-main);
	}
	tr:hover .add-note {
		opacity: 1;
	}
	td.code {
		white-space: pre;
		overflow: hidden;
		text-overflow: ellipsis;
		padding: 0 10px;
	}
	/* `.wrap` is set by the parent view ("Wrap" checkbox). */
	:global(.wrap) td.code {
		white-space: pre-wrap;
		word-break: break-all;
	}
	td.add {
		background: color-mix(in oklab, var(--atlas-vcs-added, var(--ca-semantic-success)) 14%, transparent);
	}
	td.del {
		background: color-mix(in oklab, var(--atlas-vcs-deleted, var(--ca-semantic-error)) 14%, transparent);
	}
	td.empty {
		background: color-mix(in oklab, var(--ca-text-disabled) 8%, transparent);
	}
	td.add :global(mark) {
		background: color-mix(in oklab, var(--atlas-vcs-added, var(--ca-semantic-success)) 38%, transparent);
		color: inherit;
	}
	td.del :global(mark) {
		background: color-mix(in oklab, var(--atlas-vcs-deleted, var(--ca-semantic-error)) 38%, transparent);
		color: inherit;
	}
	.mono {
		font-family: var(--mono);
	}
</style>
