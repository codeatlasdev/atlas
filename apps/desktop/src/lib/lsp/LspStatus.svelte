<script lang="ts">
	import { CircleAlert, TriangleAlert, LoaderCircle, Languages } from "@lucide/svelte";
	import { Popover } from "@codeatlasdev/ui";
	import type { LspDoc } from "#lib/lsp/client.svelte.ts";
	import { hint, dismissHint, runHintAction } from "#lib/lsp/hints.svelte.ts";
	import LspPopover from "#lib/lsp/LspPopover.svelte";
	import { uiStateOf } from "#lib/lsp/uiState.svelte.ts";

	let { doc, errors = 0, warnings = 0 }: { doc: LspDoc; errors?: number; warnings?: number } = $props();

	let ui = $derived(uiStateOf(doc));
	let mine = $derived(hint.current?.path === doc.path ? hint.current : null);
</script>

<span class="wrap">
	{#if mine}
		<span class="hint" role="status">
			<span class="msg">{mine.message}</span>
			{#if mine.action}
				<button class="act" onclick={runHintAction}>{mine.action.label}</button>
			{/if}
			<button class="x" aria-label="Dismiss" onclick={dismissHint}>×</button>
		</span>
	{/if}
	<Popover side="top" align="end" sideOffset={6}>
		{#snippet triggerChild(props)}
			<button
				{...props}
				class="lsp"
				aria-label="Language server: {ui.label}"
				title={ui.summary}
				data-kind={ui.kind}
			>
				{#if ui.tone === "busy"}<LoaderCircle size={12} class="spin" />{:else}<Languages size={12} />{/if}
				<span class="dot {ui.tone}"></span>
				<span class="label">{ui.label}</span>
			</button>
		{/snippet}
		<LspPopover {doc} {ui} />
	</Popover>
	{#if errors > 0}
		<span class="count err" title="{errors} error{errors === 1 ? '' : 's'}"
			><CircleAlert size={12} />{errors}</span
		>
	{/if}
	{#if warnings > 0}
		<span class="count warn" title="{warnings} warning{warnings === 1 ? '' : 's'}"
			><TriangleAlert size={12} />{warnings}</span
		>
	{/if}
</span>

<style>
	.wrap {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		min-width: 0;
	}
	.lsp {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 6px;
		border: 0;
		border-radius: 5px;
		background: none;
		font-size: 11px;
		color: var(--ca-text-secondary);
		max-width: 280px;
		min-width: 0;
		cursor: pointer;
	}
	.lsp:hover,
	.lsp[data-state="open"] {
		background: var(--ca-action-hover);
	}
	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dot {
		flex: none;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-disabled);
	}
	.dot.ok {
		background: var(--atlas-state-success, var(--ca-semantic-success));
	}
	.dot.error {
		background: var(--atlas-state-error, var(--ca-semantic-error));
	}
	.dot.warn,
	.dot.busy {
		background: var(--atlas-state-warning, var(--ca-semantic-warning));
	}
	.hint {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		max-width: 520px;
		padding: 2px 6px;
		border-radius: 5px;
		font-size: 11px;
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.msg {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.act,
	.x {
		flex: none;
		border: 0;
		background: none;
		font: inherit;
		cursor: pointer;
		color: var(--ca-text-secondary);
	}
	.act {
		color: var(--ca-text-primary);
		text-decoration: underline;
	}
	.count {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		padding: 2px 4px;
	}
	.err {
		color: var(--ca-semantic-error);
	}
	.warn {
		color: var(--ca-semantic-warning);
	}
	:global(.spin) {
		animation: lsp-spin 1s linear infinite;
	}
	@keyframes lsp-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
