<script lang="ts">
	/** One hunk as a table: line numbers, comment gutter, highlighted code, review notes and the draft box. */
	import { MessageSquarePlus } from "@lucide/svelte";
	import DiffNotes from "./DiffNotes.svelte";
	import type { DiffHunk } from "#lib/api.ts";
	import type { DiffComment } from "#lib/stores/workspace.svelte.ts";

	export interface Draft {
		path: string;
		line: number;
		side: "old" | "new";
		text: string;
	}

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
</script>

<table>
	<tbody>
		{#each hunk.lines as l, li (li)}
			{@const side = l.kind === "del" ? "old" : "new"}
			{@const no = side === "old" ? l.old_no : l.new_no}
			{@const html = tokens?.[li]}
			<tr class={l.kind}>
				<td class="no">{l.old_no ?? ""}</td>
				<td class="no">{l.new_no ?? ""}</td>
				<td class="gut">
					{#if no !== undefined}
						<button
							class="add-note"
							title="Comment (c)"
							onclick={() => (draft = { path, line: no, side, text: "" })}
						>
							<MessageSquarePlus size={11} />
						</button>
					{/if}
				</td>
				<td class="code mono">
					<span class="sign">{l.kind === "add" ? "+" : l.kind === "del" ? "-" : " "}</span
					>{#if html !== undefined}{@html html}{:else}{l.text}{/if}
				</td>
			</tr>
			<DiffNotes {path} {side} line={no} cols={4} {comments} bind:draft {onsave} {onresolve} />
		{/each}
	</tbody>
</table>

<style>
	table {
		width: 100%;
		border-collapse: collapse;
		table-layout: fixed;
	}
	td.no {
		width: 44px;
		text-align: right;
		padding: 0 6px;
		color: var(--ca-text-disabled);
		font-variant-numeric: tabular-nums;
		user-select: none;
	}
	td.gut {
		width: 18px;
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
		padding-right: 10px;
	}
	/* `.wrap` is set by the parent view ("Wrap" checkbox). */
	:global(.wrap) td.code {
		white-space: pre-wrap;
		word-break: break-all;
	}
	.sign {
		user-select: none;
		padding-right: 4px;
	}
	tr.add {
		background: color-mix(in oklab, var(--atlas-vcs-added, var(--ca-semantic-success)) 14%, transparent);
	}
	tr.add .sign {
		color: var(--atlas-vcs-added, var(--ca-semantic-success));
	}
	tr.del {
		background: color-mix(in oklab, var(--atlas-vcs-deleted, var(--ca-semantic-error)) 14%, transparent);
	}
	tr.del .sign {
		color: var(--atlas-vcs-deleted, var(--ca-semantic-error));
	}
	tr.ctx .sign {
		color: var(--ca-text-disabled);
	}
	.mono {
		font-family: var(--mono);
	}
</style>
