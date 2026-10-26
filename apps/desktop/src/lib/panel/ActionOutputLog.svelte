<script lang="ts">
	import { toast } from "@codeatlasdev/ui";
	import { api } from "#lib/api.ts";
	import { resolvePath } from "#lib/logs/links.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import VirtualList from "#lib/components/VirtualList.svelte";
	import type { Run, RunLine } from "./actionRuns.ts";
	import { segments, type Seg } from "./actionSegments.ts";

	/** Output of one run: tail window, follow-while-live scrolling and clickable URLs/paths. */
	let {
		run,
		rev,
		active,
		wtId,
		root,
	}: {
		run: Run;
		/** Bumped by the runs store on every change of the (non-reactive) line rings. */
		rev: number | undefined;
		/** The drawer is open: only then does a live run keep the view pinned to its end. */
		active: boolean;
		wtId: string | undefined;
		/** Absolute root used to resolve relative file links. */
		root: string;
	} = $props();

	/** The whole ring (up to 5000 lines) is windowed by VirtualList: only the rows on screen are in the DOM. */
	let lines = $derived.by(() => {
		void rev;
		return { items: [...run.lines.items], first: run.lines.base };
	});

	let list: VirtualList<RunLine> | undefined = $state();

	// Switching runs starts at the end (what just happened), following again.
	$effect(() => {
		void run.id;
		list?.scrollToEnd();
	});

	async function openLink(e: MouseEvent, seg: Seg) {
		const link = seg.link;
		if (!link) return;
		e.preventDefault();
		try {
			if (link.kind === "url" || link.kind === "osc") {
				if (/^https?:/i.test(link.target) && wtId) await openPreview(wtId, link.target, e.shiftKey);
				else await api.openUrl(link.target);
				return;
			}
			const rel = resolvePath(link.target, root, null);
			if (rel && wtId) openFile(wtId, rel, link.line, link.col);
			else toast.error(`${link.target} is outside the project`);
		} catch (err) {
			toast.error(err instanceof Error ? err.message : String(err));
		}
	}
</script>

<VirtualList
	bind:this={list}
	class="action-out mono"
	items={lines.items}
	key={(_, i) => String(lines.first + i)}
	estimate={17}
	stickBottom={active && run.status === "running"}
>
	{#snippet header()}
		{#if run.truncated > 0}
			<div class="note">
				{run.truncated.toLocaleString()} earlier lines were not kept (attached mid-run); full output is in .logs/.
			</div>
		{/if}
		{#if lines.items.length === 0}
			<div class="note">{run.status === "running" ? "Waiting for output…" : "No output."}</div>
		{/if}
	{/snippet}
	{#snippet item(l)}
		<div class="ln">
			{#each segments(l.text) as s}{#if s.link}<a
						href={s.link.target}
						class="lnk"
						style={s.css}
						title={s.link.kind === "file" ? "Open file" : "Open link (⇧ for the browser)"}
						onclick={(e) => openLink(e, s)}>{s.text}</a
					>{:else}<span style={s.css}>{s.text}</span>{/if}{/each}
		</div>
	{/snippet}
</VirtualList>

<style>
	:global(.vl.action-out) {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--ca-surface-sunken, var(--ca-surface-raised));
		border: 1px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		line-height: 1.5;
		user-select: text;
		-webkit-user-select: text;
	}
	.ln {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		min-height: 1.5em;
	}
	.lnk {
		color: var(--ca-semantic-info);
		text-decoration: underline;
		text-underline-offset: 2px;
		cursor: pointer;
	}
	.note {
		padding: 2px 0 6px;
		font-family: inherit;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
</style>
