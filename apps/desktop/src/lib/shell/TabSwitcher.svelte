<script lang="ts">
	import {
		SquareTerminal,
		LayoutGrid,
		ScrollText,
		FileCode2,
		FileText,
		FileDiff,
		GitPullRequest,
		Globe,
		Bot,
	} from "@lucide/svelte";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { switcher } from "#lib/shell/switcher.svelte.ts";

	const ICONS = {
		terminal: SquareTerminal,
		services: LayoutGrid,
		logs: ScrollText,
		config: FileCode2,
		editor: FileText,
		diff: FileDiff,
		pr: GitPullRequest,
		browser: Globe,
	} as const;

	let items = $derived(
		(switcher.s?.ids ?? []).flatMap((id) => {
			const t = ws.tab(id);
			if (!t) return [];
			const title =
				t.kind === "services" ? "Panel" : t.customLabel ? t.label : (t.pty && ws.titles[t.pty]) || t.label;
			return [{ id, title, Icon: t.agent ? Bot : (ICONS[t.kind as keyof typeof ICONS] ?? SquareTerminal) }];
		}),
	);
</script>

{#if switcher.s}
	<div class="switcher" role="listbox" aria-label="Recent tabs">
		{#each items as it, i (it.id)}
			<div class="row" class:on={i === switcher.s.index} role="option" aria-selected={i === switcher.s.index}>
				<it.Icon size={14} />
				<span>{it.title}</span>
			</div>
		{/each}
	</div>
{/if}

<style>
	.switcher {
		position: fixed;
		top: 22%;
		left: 50%;
		transform: translateX(-50%);
		z-index: 60;
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 260px;
		max-width: 420px;
		padding: 6px;
		border-radius: 12px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-surface-base));
		box-shadow: 0 20px 60px rgb(0 0 0 / 0.25);
		pointer-events: none;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border-radius: 8px;
		font-size: 13px;
		color: var(--ca-text-secondary);
	}
	.row span {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row.on {
		color: var(--ca-text-primary);
		background: var(--ca-action-selected, var(--ca-action-hover));
	}
</style>
