<script lang="ts">
	/** Review notes under one diff line plus the draft box, as table rows spanning `cols` columns (shared by the unified and split tables). */
	import { Check } from "@lucide/svelte";
	import type { DiffComment } from "#lib/stores/workspace.svelte.ts";
	import type { Draft } from "./DiffTable.svelte";

	let {
		path,
		side,
		line,
		cols,
		comments,
		draft = $bindable(),
		onsave,
		onresolve,
	}: {
		path: string;
		side: "old" | "new";
		line: number | undefined;
		cols: number;
		comments: DiffComment[];
		draft: Draft | null;
		onsave: () => void;
		onresolve: (id: string, resolved: boolean) => void;
	} = $props();

	let notes = $derived(
		line === undefined ? [] : comments.filter((c) => c.path === path && c.side === side && c.line === line),
	);
</script>

{#each notes as c (c.id)}
	<tr class="note-row">
		<td colspan={cols}>
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
{#if draft && draft.path === path && draft.line === line && draft.side === side}
	<tr class="note-row">
		<td colspan={cols}>
			<!-- svelte-ignore a11y_autofocus -->
			<textarea
				autofocus
				placeholder="Comment for the agent (⌘Enter to save, Esc to cancel)"
				bind:value={draft.text}
				onkeydown={(e) => {
					if (e.key === "Enter" && e.metaKey) onsave();
					if (e.key === "Escape") draft = null;
				}}></textarea>
		</td>
	</tr>
{/if}

<style>
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
	.dim {
		color: var(--ca-text-secondary);
		font-weight: 400;
	}
</style>
