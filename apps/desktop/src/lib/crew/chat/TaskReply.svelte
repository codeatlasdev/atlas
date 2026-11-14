<script lang="ts">
	/**
	 * TaskReply — the body of a finished task: the automation note, then either the ask card, the summary with
	 * "See full reply", a long reply split into chat messages, or one reply that folds past a dozen lines; and the
	 * usage footer. Fold / full / dismissed flags live in ChatState.expandedResults.
	 */
	import { ChevronDown, ChevronUp, Check } from "@lucide/svelte";
	import type { CrewmateTask } from "#lib/crew.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";
	import { msgIn } from "#lib/crew/motion.ts";
	import { replyParts } from "#lib/crew/replyParts.ts";
	import AskCard from "./AskCard.svelte";
	import type { ChatState } from "./chat.svelte.ts";
	import { parseAsk } from "./parseAsk.ts";
	import { collapsedText, expandKey, fmtElapsed, fmtTokens, shouldCollapse } from "./taskView.ts";

	let { task: t, chat }: { task: CrewmateTask; chat: ChatState } = $props();

	const html = (text: string) => renderMarkdown(text, "", false);

	const raw = $derived(t.result ?? "Done");
	const ask = $derived(parseAsk(t.result));
	const dismissed = $derived(chat.isDismissed(t.id));
	const folded = $derived(shouldCollapse(raw) && !chat.expandedResults.has(t.id));
	const shownFull = $derived(chat.expandedResults.has(expandKey.full(t.id)));
</script>

{#if t.action?.kind === "add_trigger" && t.answer}
	<div class="task-note">
		{#if t.answer === "add"}
			<Check size={12} aria-hidden="true" /> Automation added
		{:else}
			Not added
		{/if}
	</div>
{/if}
{#if ask && !dismissed}
	<AskCard
		{ask}
		onanswer={(text) => void chat.sendText(text)}
		ondismiss={() => chat.setExpanded(expandKey.dismissed(t.id), true)}
	/>
{:else if !ask && !dismissed}
	{#if t.summarized && !shownFull}
		<div class="md-result ca-selectable">{@html html(raw)}</div>
		<button class="show-more" onclick={() => chat.setExpanded(expandKey.full(t.id), true)}
			>See full reply</button
		>
	{:else if t.summarized && shownFull}
		<div class="md-result ca-selectable">{@html html(t.reply_full ?? raw)}</div>
		<button class="show-more" onclick={() => chat.setExpanded(expandKey.full(t.id), false)}
			>Show summary</button
		>
	{:else}
		{@const parts = replyParts(raw)}
		{#if parts.length > 1}
			<!-- A long reply as a few chat messages, one per section, instead of one long document. -->
			{#each parts as part, pi (pi)}
				<div class="md-result ca-selectable reply-part" in:msgIn={{ delay: pi * 90 }}>{@html html(part)}</div>
			{/each}
		{:else}
			<div class="md-result ca-selectable" class:collapsed={folded}>
				{@html html(folded ? collapsedText(raw) : raw)}
			</div>
			{#if shouldCollapse(raw)}
				<button class="show-more" onclick={() => chat.setExpanded(t.id, folded)}>
					{folded ? "Show more" : "Show less"}
					{#if folded}<ChevronDown size={12} aria-hidden="true" />{:else}<ChevronUp
							size={12}
							aria-hidden="true"
						/>{/if}
				</button>
			{/if}
		{/if}
	{/if}
{/if}
{#if t.usage}
	<div class="usage-footer">
		{#if t.usage.input_tokens || t.usage.output_tokens}
			<span>{fmtTokens(t.usage.input_tokens ?? 0)}in · {fmtTokens(t.usage.output_tokens ?? 0)}out</span>
		{/if}
		{#if t.usage.cost_usd}<span>${t.usage.cost_usd.toFixed(4)}</span>{/if}
		{#if t.usage.duration_ms}<span>{fmtElapsed(t.usage.duration_ms)}</span>{/if}
		{#if t.usage.model}<span class="usage-model">{t.usage.model}</span>{/if}
	</div>
{/if}

<style>
	/* Consecutive messages of one reply sit close together, like a burst of chat messages. */
	.reply-part + .reply-part {
		margin-top: 6px;
	}
	/* The crewmate's reply sits in the same bubble as every other message (loose text on the page read as a document,
	   not a conversation). One size throughout: the markdown renderer's paragraphs came out smaller than lists. */
	.md-result {
		font-size: 15px;
		line-height: 1.55;
		color: var(--ca-text-primary);
		word-break: break-word;
		max-width: 72%;
		width: fit-content;
		padding: 12px 16px;
		border-radius: 18px;
		border-bottom-left-radius: 4px;
		background: var(--ca-surface-sunken);
	}
	.md-result :global(:first-child) {
		margin-top: 0;
	}
	.md-result :global(:last-child) {
		margin-bottom: 0;
	}
	.md-result :global(p),
	.md-result :global(li) {
		font-size: inherit;
	}
	.md-result :global(h1),
	.md-result :global(h2),
	.md-result :global(h3) {
		font-size: 14px;
		font-weight: 600;
		margin: 0.8em 0 0.4em;
	}
	.md-result :global(p) {
		margin: 0.4em 0;
	}
	.md-result :global(ul),
	.md-result :global(ol) {
		padding-left: 1.4em;
		margin: 0.4em 0;
	}
	.md-result :global(li) {
		margin: 0.2em 0;
	}
	.md-result :global(pre.code) {
		position: relative;
		background: color-mix(in oklab, var(--ca-surface-sunken) 80%, transparent);
		border-radius: 6px;
		padding: 10px 12px;
		overflow-x: auto;
		font-size: 12px;
		margin: 0.6em 0;
	}
	.md-result :global(code) {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		border-radius: 3px;
		padding: 1px 4px;
	}
	.md-result :global(pre.code code) {
		background: none;
		padding: 0;
	}
	.md-result :global(table) {
		border-collapse: collapse;
		width: 100%;
		font-size: 12px;
		margin: 0.6em 0;
	}
	.md-result :global(th),
	.md-result :global(td) {
		border: 0.5px solid var(--ca-outline-subtle);
		padding: 4px 8px;
		text-align: left;
	}
	.md-result :global(th) {
		background: color-mix(in oklab, var(--ca-surface-sunken) 60%, transparent);
		font-weight: 600;
	}
	.show-more {
		font-size: 11.5px;
		color: var(--ca-brand-main);
		background: none;
		border: none;
		padding: 2px 0;
		cursor: default;
		margin-top: 2px;
	}
	.usage-footer {
		display: none;
		align-items: center;
		gap: 8px;
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		padding: 2px 0;
		flex-wrap: wrap;
	}
	:global(.msg-in:hover) .usage-footer {
		display: flex;
	}
	.usage-model {
		font-style: italic;
	}
</style>
