<script lang="ts">
	import { modClick } from "#lib/keys.ts";
	import type { LogLine } from "#lib/logs/model.ts";
	import { hms } from "#lib/logs/text.ts";
	import { svcColor } from "#lib/logs/svcColor.svelte.ts";
	import { segments, type Seg } from "#lib/logs/segments.ts";
	import { ROW } from "#lib/logs/viewport.svelte.ts";

	let {
		line: l,
		current,
		matcher,
		wrap,
		showTime,
		showService,
		onlink,
	}: {
		line: LogLine;
		/** This row is the current search/jump target. */
		current: boolean;
		matcher: RegExp | null;
		wrap: boolean;
		showTime: boolean;
		showService: boolean;
		onlink: (e: MouseEvent, seg: Seg, line: LogLine) => void;
	} = $props();
</script>

{#if l.mark !== undefined}
	<div class="mark" style="--svc:{svcColor(l.name)}">
		<span>{hms(l.ts)} · {l.mark}</span>
	</div>
{:else}
	<div
		class="row lv-{l.level}"
		class:err={l.stream === "stderr"}
		class:wrap
		class:cur={current}
		style={wrap ? "" : `height:${ROW}px`}
	>
		{#if showTime}<span class="ts">{hms(l.ts)}</span>{/if}
		{#if showService}<span class="svc" style="--svc:{svcColor(l.name)}">{l.name}</span>{/if}
		<span class="txt"
			>{#each segments(l, matcher, current) as p, i (i)}{#if p.link}<span
						class="link"
						class:hit={p.hit === 1}
						class:cur={p.hit === 2}
						style={p.css}
						title={p.link.kind === "file"
							? `${modClick()} to open ${p.link.target}${p.link.line ? `:${p.link.line}` : ""}`
							: `${modClick()} to open ${p.link.target}`}
						role="link"
						tabindex="-1"
						onclick={(e) => onlink(e, p, l)}
						onkeydown={() => {}}>{p.text}</span
					>{:else if p.hit}<mark class:cur={p.hit === 2} style={p.css}>{p.text}</mark>{:else if p.css}<span
						style={p.css}>{p.text}</span
					>{:else}{p.text}{/if}{/each}</span
		>
	</div>
{/if}

<style>
	.row {
		line-height: 18px;
		padding: 0 16px;
		white-space: pre;
		display: flex;
		gap: 10px;
		font-variant-ligatures: none;
	}
	.row.wrap {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.row:hover {
		background: var(--ca-action-hover);
	}
	.row.cur {
		background: color-mix(in oklab, var(--ca-brand-fill) 14%, transparent);
	}
	.ts,
	.svc {
		user-select: none;
		-webkit-user-select: none;
	}
	.ts {
		color: var(--ca-text-secondary);
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}
	.svc {
		flex-shrink: 0;
		width: 92px;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--svc);
	}
	.txt {
		color: var(--ca-text-primary);
		min-width: 0;
	}
	.lv-warn .txt {
		color: var(--ca-semantic-warning);
	}
	.lv-error .txt {
		color: var(--ca-semantic-error);
	}
	mark,
	.link.hit {
		background: var(--cm-search-match, color-mix(in oklab, var(--ca-semantic-warning) 40%, transparent));
		color: inherit;
		border-radius: 2px;
	}
	mark.cur,
	.link.cur {
		background: var(
			--cm-search-match-active,
			color-mix(in oklab, var(--ca-semantic-warning) 70%, transparent)
		);
		outline: 1px solid var(--ca-outline-strong);
		color: inherit;
		border-radius: 2px;
	}
	.mark {
		display: flex;
		align-items: center;
		height: 18px;
		padding: 0 16px;
		gap: 10px;
		color: var(--svc);
		font-size: 11px;
		user-select: none;
		-webkit-user-select: none;
	}
	.mark::before,
	.mark::after {
		content: "";
		flex: 1;
		border-top: 1px dashed color-mix(in oklab, currentColor 45%, transparent);
	}
</style>
