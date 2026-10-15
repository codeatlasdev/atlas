<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import type { LspDoc } from "#lib/lsp/client.svelte.ts";
	import { runAction } from "#lib/lsp/run.ts";
	import type { Action, LspUiState } from "#lib/lsp/status.ts";

	let { doc, ui }: { doc: LspDoc; ui: LspUiState } = $props();

	let busy = $state<string | null>(null);

	async function run(a: Action) {
		busy = a.id;
		try {
			await runAction(a.id, doc.path, a.payload);
		} finally {
			busy = null;
		}
	}
</script>

<div class="pop" role="group" aria-label="Language server">
	<p class="summary">{ui.summary}</p>
	{#if ui.detail}<p class="detail">{ui.detail}</p>{/if}
	{#if ui.kind === "host-old" && ui.ptys.length > 0}
		<ul class="ptys" aria-label="Running terminals">
			{#each ui.ptys.slice(0, 6) as p}
				<li>
					<span class="t">{p.title || "Terminal"}</span><span class="cwd mono" dir="rtl"
						><bdi dir="ltr">{p.cwd}</bdi></span
					>
				</li>
			{/each}
			{#if ui.ptys.length > 6}<li class="more">and {ui.ptys.length - 6} more</li>{/if}
		</ul>
	{/if}
	{#if ui.kind === "missing-toolchain" && ui.command}
		<pre class="cmd mono">{ui.command}</pre>
	{/if}
	{#if ui.tail?.length}
		<pre class="tail mono" aria-label="Last server output">{ui.tail.slice(-8).join("\n")}</pre>
	{/if}
	{#if doc.projectWarning}<p class="warn">{doc.projectWarning}</p>{/if}
	{#if ui.actions.length}
		<div class="actions">
			{#each ui.actions as a (a.id)}
				<Button
					size="sm"
					variant={a.primary ? "filled" : "ghost"}
					loading={busy === a.id}
					disabled={busy !== null}
					onclick={() => run(a)}
				>
					{a.label}
				</Button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.pop {
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 340px;
		max-width: calc(100vw - 32px);
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-primary);
	}
	p {
		margin: 0;
	}
	.detail {
		color: var(--ca-text-secondary);
	}
	.warn {
		color: var(--ca-semantic-warning);
	}
	.ptys {
		margin: 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.ptys li {
		display: flex;
		gap: 8px;
		justify-content: space-between;
		min-width: 0;
	}
	.t {
		font-weight: 500;
		white-space: nowrap;
	}
	.cwd {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.more {
		color: var(--ca-text-secondary);
	}
	.cmd,
	.tail {
		margin: 0;
		padding: 6px 8px;
		border-radius: 6px;
		background: var(--ca-action-hover);
		font-size: 11px;
		overflow-x: auto;
		white-space: pre;
	}
	.tail {
		max-height: 130px;
		overflow-y: auto;
		color: var(--ca-text-secondary);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 2px;
	}
</style>
