<script lang="ts" module>
	export interface PaletteItem {
		id: string;
		label: string;
		group: string;
		description?: string;
		shortcut?: string;
		/** Inline SVG markup shown before the label. */
		icon?: string;
		/** Repo id: shows that project's identity tile instead of `icon`. */
		project?: string;
		/** `split`: Shift-Enter (open beside). */
		run: (split: boolean) => void;
	}
</script>

<script lang="ts">
	import { tick } from "svelte";
	import { fuzzy, highlight } from "#lib/fuzzy.ts";
	import { CornerDownLeft, ArrowUpDown } from "@lucide/svelte";
	import { Kbd } from "@codeatlasdev/ui";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";

	let {
		open = $bindable(false),
		placeholder = "Search",
		items,
		dynamic,
	}: {
		open?: boolean;
		placeholder?: string;
		/** Static items; filtered by fuzzy match on label + description. */
		items: (query: string) => PaletteItem[];
		/** Extra items built from the raw query (e.g. "Create worktree <q>"). */
		dynamic?: (query: string) => PaletteItem[];
	} = $props();

	let query = $state("");
	let sel = $state(0);
	let input: HTMLInputElement | undefined = $state();

	let list: HTMLDivElement | undefined = $state();
	let q = $derived(query.trim());

	let results = $derived.by(() => {
		const base = items(q);
		const scored = q
			? base
					.map((it) => ({ it, s: fuzzy(q, it.label, it.description) }))
					.filter((x) => x.s > 0)
					.sort((a, b) => b.s - a.s)
					.map((x) => x.it)
			: base;
		return [...scored.slice(0, 80), ...(dynamic?.(q) ?? [])];
	});

	$effect(() => {
		if (open) {
			query = "";
			sel = 0;
			void tick().then(() => input?.focus());
		}
	});

	$effect(() => {
		sel;
		void tick().then(() =>
			list?.querySelector<HTMLElement>(".row.sel")?.scrollIntoView({ block: "nearest" }),
		);
	});

	function run(i: number, split: boolean) {
		const it = results[i];
		if (!it) return;
		open = false;
		it.run(split);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === "ArrowDown") sel = Math.min(sel + 1, results.length - 1);
		else if (e.key === "ArrowUp") sel = Math.max(sel - 1, 0);
		else if (e.key === "Enter") run(sel, e.shiftKey);
		else if (e.key === "Escape") open = false;
		else if (e.metaKey && /^[1-6]$/.test(e.key)) run(Number(e.key) - 1, false);
		else return;
		e.preventDefault();
		e.stopPropagation();
	}
</script>

{#if open}
	<div class="backdrop" role="presentation" onclick={() => (open = false)}></div>
	<div class="palette" role="dialog" aria-label={placeholder}>
		<input bind:this={input} bind:value={query} {placeholder} {onkeydown} oninput={() => (sel = 0)} />
		<div class="list" bind:this={list}>
			{#each results as it, i (it.group + it.id)}
				{#if i === 0 || results[i - 1].group !== it.group}
					<div class="group">{it.group}</div>
				{/if}
				<button
					class="row"
					class:sel={i === sel}
					onmouseenter={() => (sel = i)}
					onclick={(e) => run(i, e.shiftKey)}
				>
					{#if it.project}<span class="ico"><ProjectIcon id={it.project} size={18} /></span
						>{:else if it.icon}<span class="ico">{@html it.icon}</span>{/if}
					<span class="label"
						>{#each highlight(q, it.label) as r, k (k)}{#if r.hit}<mark>{r.text}</mark
								>{:else}{r.text}{/if}{/each}</span
					>
					{#if it.description}<span class="desc" dir="rtl"><bdi dir="ltr">{it.description}</bdi></span>{/if}
					{#if it.shortcut}<Kbd>{it.shortcut}</Kbd>{/if}
				</button>
			{:else}
				<div class="empty">
					{q ? `Nothing matches “${q}”` : "Nothing to show"}
				</div>
			{/each}
		</div>
		<div class="foot">
			<span><ArrowUpDown size={11} /> navigate</span>
			<span><CornerDownLeft size={11} /> select</span>
			<span><Kbd>⇧↩</Kbd> open beside</span>
			<span class="end"><Kbd>⎋</Kbd> close</span>
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 90;
	}
	.palette {
		position: fixed;
		top: 14vh;
		left: 50%;
		transform: translateX(-50%);
		width: min(640px, 90vw);
		max-height: 60vh;
		display: flex;
		flex-direction: column;
		z-index: 91;
		border-radius: 12px;
		background: var(--ca-surface-raised, var(--ca-surface-base));
		border: 0.5px solid var(--ca-outline-subtle);
		box-shadow: 0 20px 60px rgb(0 0 0 / 0.25);
		overflow: hidden;
		animation: pop 110ms ease-out;
	}
	@keyframes pop {
		from {
			opacity: 0;
			transform: translateX(-50%) translateY(-4px) scale(0.985);
		}
	}
	input {
		font: inherit;
		font-size: 15px;
		padding: 14px 16px;
		border: 0;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-primary);
		outline: none;
	}
	.list {
		overflow-y: auto;
		padding: 6px;
	}
	.group {
		padding: 8px 10px 4px;
		font-size: 11px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.row {
		display: flex;
		align-items: baseline;
		gap: 10px;
		width: 100%;
		padding: 7px 10px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 13px;
		text-align: left;
	}
	.row.sel {
		background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
	}
	mark {
		background: none;
		color: var(--ca-brand-main);
		font-weight: 650;
	}
	.ico {
		display: inline-flex;
		align-self: center;
		flex-shrink: 0;
		margin-right: -4px;
	}
	.ico :global(svg) {
		width: 16px;
		height: 16px;
	}
	.label {
		white-space: nowrap;
	}
	.desc {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		text-align: left;
	}
	.row :global(.ca-kbd) {
		margin-left: auto;
		flex-shrink: 0;
	}
	.foot {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 7px 14px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.foot span {
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}
	.foot .end {
		margin-left: auto;
	}
	.empty {
		padding: 16px;
		text-align: center;
		color: var(--ca-text-secondary);
		font-size: 13px;
	}
</style>
