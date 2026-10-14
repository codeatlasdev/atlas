<script lang="ts">
	/** One hunk as a table: line numbers, comment gutter, highlighted code, review notes and the draft box. */
	import { Check, MessageSquarePlus } from "@lucide/svelte";
	import { kbd, primaryKey } from "#lib/keys.ts";
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

	const notesAt = (side: "old" | "new", line: number | undefined) =>
		line === undefined ? [] : comments.filter((c) => c.path === path && c.side === side && c.line === line);
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
			{#each notesAt(side, no) as c (c.id)}
				<tr class="note-row">
					<td colspan="4">
						<div class="note" class:resolved={c.resolved}>
							<span>{c.text}</span>
							{#if c.sent}<span class="dim">sent</span>{/if}
							<button onclick={() => onresolve(c.id, !c.resolved)}>
								<Check size={11} />
								{c.resolved ? "Reopen" : "Resolve"}
							</button>
						</div>
					</td>
				</tr>
			{/each}
			{#if draft && draft.path === path && draft.line === no && draft.side === side}
				<tr class="note-row">
					<td colspan="4">
						<!-- svelte-ignore a11y_autofocus -->
						<textarea
							autofocus
							placeholder={`Comment for the agent (${kbd("cmd-enter")} to save, Esc to cancel)`}
							bind:value={draft.text}
							onkeydown={(e) => {
								if (e.key === "Enter" && primaryKey(e)) onsave();
								if (e.key === "Escape") draft = null;
							}}></textarea>
					</td>
				</tr>
			{/if}
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
	.note-row td {
		padding: 4px 10px 4px 110px;
	}
	.note {
		display: flex;
		gap: 10px;
		align-items: center;
		padding: 6px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-surface-base));
		font-family: system-ui;
	}
	.note.resolved {
		color: var(--ca-text-secondary);
	}
	.note button {
		margin-left: auto;
		display: inline-flex;
		gap: 3px;
		align-items: center;
		border: 0;
		background: none;
		font: inherit;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	textarea {
		width: 100%;
		min-height: 56px;
		font: inherit;
		font-family: system-ui;
		padding: 6px 8px;
		border-radius: 6px;
		border: 1px solid var(--ca-brand-main);
		background: var(--ca-surface-base);
		color: inherit;
	}
	.mono {
		font-family: var(--mono);
	}
	.dim {
		color: var(--ca-text-secondary);
		font-weight: 400;
	}
</style>
