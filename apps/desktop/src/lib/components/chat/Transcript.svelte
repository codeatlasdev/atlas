<script lang="ts">
	/** The chat transcript: user/assistant/note/error messages, folded thinking and tool calls, ask prompts. */
	import { tick } from "svelte";
	import { primaryKey } from "#lib/keys.ts";
	import { Spinner } from "@codeatlasdev/ui";
	import { ChevronDown, ChevronRight, Wrench } from "@lucide/svelte";
	import { marked } from "marked";
	import type { ChatState, UiRequest } from "#lib/rpc.ts";
	import { argsLine } from "./composer.ts";
	import ChatTextarea from "./ChatTextarea.svelte";

	let {
		chat,
		items,
		onanswer,
	}: {
		chat: ChatState;
		/** Items to show (the whole transcript, or the search matches). */
		items: ChatState["items"];
		onanswer: (u: UiRequest, value: unknown) => void;
	} = $props();

	let scroller: HTMLDivElement | undefined = $state();
	let open = $state<Record<string, boolean>>({});

	const md = (s: string) => marked.parse(s, { async: false, breaks: true });

	/** Is the view within reach of the end (so new content should keep following)? */
	export const nearBottom = () =>
		!scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;

	export async function scrollDown() {
		await tick();
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}
</script>

<div class="transcript" bind:this={scroller}>
	{#if !chat.ready}<div class="center"><Spinner /> Starting omp…</div>{/if}
	{#each items as it (it.id)}
		{#if it.kind === "user"}
			<div class="msg user ca-selectable">
				{it.text}{#if it.images}<span class="dim"> · {it.images} image(s)</span>{/if}
			</div>
		{:else if it.kind === "assistant"}
			<div class="msg assistant">
				{#if it.thinking}
					<button class="fold" onclick={() => (open[`th-${it.id}`] = !open[`th-${it.id}`])}>
						{#if open[`th-${it.id}`]}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if} Thinking
					</button>
					{#if open[`th-${it.id}`]}<pre class="thinking ca-selectable">{it.thinking}</pre>{/if}
				{/if}
				{#if it.text}<div class="md ca-selectable">{@html md(it.text)}</div>{/if}
				{#each it.tools as id (id)}
					{@const tc = chat.tools[id]}
					{#if tc}
						<div class="tool {tc.status}">
							<button class="fold" onclick={() => (open[id] = !open[id])}>
								{#if tc.status === "running"}<Spinner size={11} />{:else}<Wrench size={11} />{/if}
								<b>{tc.name}</b> <span class="mono">{argsLine(tc.args)}</span>
							</button>
							{#if open[id] && tc.result}<pre class="ca-selectable">{tc.result}</pre>{/if}
						</div>
					{/if}
				{/each}
				{#if it.streaming && !it.text && !it.tools.length}<Spinner size={12} />{/if}
			</div>
		{:else if it.kind === "note"}
			<div class="msg note">{it.text}</div>
		{:else}
			<div class="msg error ca-selectable">{it.text}</div>
		{/if}
	{/each}
	{#each chat.ui as u (u.id)}
		<div class="ask">
			<b>{u.title}</b>
			{#if u.message}<p>{u.message}</p>{/if}
			{#if u.method === "select"}
				<div class="opts">
					{#each u.options ?? [] as o (o)}<button onclick={() => onanswer(u, o)}>{o}</button>{/each}
				</div>
			{:else if u.method === "confirm"}
				<div class="opts">
					<button onclick={() => onanswer(u, true)}>Yes</button>
					<button onclick={() => onanswer(u, false)}>No</button>
				</div>
			{:else}
				<ChatTextarea
					placeholder={u.placeholder ?? ""}
					onkeydown={(e) => {
						if (e.key === "Enter" && (primaryKey(e) || u.method === "input")) {
							e.preventDefault();
							onanswer(u, e.currentTarget.value);
						}
					}}
				/>
			{/if}
			<button class="cancel" onclick={() => onanswer(u, undefined)}>Dismiss</button>
		</div>
	{/each}
</div>

<style>
	.transcript {
		flex: 1;
		overflow-y: auto;
		padding: 12px 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.center {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--ca-text-secondary);
	}
	.msg {
		max-width: 860px;
		line-height: 1.55;
	}
	.user {
		align-self: flex-end;
		white-space: pre-wrap;
		padding: 8px 12px;
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
	}
	.note {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		border-left: 2px solid var(--ca-outline-subtle);
		padding-left: 8px;
	}
	.error {
		color: var(--ca-semantic-error, #d33);
		white-space: pre-wrap;
	}
	.md :global(pre) {
		overflow-x: auto;
		padding: 8px;
		border-radius: 6px;
		background: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
	}
	.md :global(p) {
		margin: 0.4em 0;
	}
	.fold {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 0;
		text-align: left;
		max-width: 100%;
	}
	.thinking,
	.tool pre {
		margin: 4px 0 4px 16px;
		max-height: 260px;
		overflow: auto;
		font-size: 11.5px;
		white-space: pre-wrap;
		color: var(--ca-text-secondary);
	}
	.tool {
		border-left: 2px solid var(--ca-outline-subtle);
		padding-left: 8px;
		margin: 4px 0;
	}
	.tool.error {
		border-left-color: var(--ca-semantic-error, #d33);
	}
	.mono {
		font-family: var(--mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dim {
		color: var(--ca-text-secondary);
		font-size: 11.5px;
	}
	.ask {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border-radius: 10px;
		border: 1px solid var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, transparent);
		max-width: 640px;
	}
	.ask p {
		margin: 0;
		white-space: pre-wrap;
	}
	.opts {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.opts button,
	.cancel {
		font: inherit;
		font-size: 12px;
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: inherit;
	}
	.cancel {
		align-self: flex-start;
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
	}
</style>
