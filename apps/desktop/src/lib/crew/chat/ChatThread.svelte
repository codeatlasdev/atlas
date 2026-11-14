<script lang="ts">
	/**
	 * ChatThread — the scrolling list of tasks, oldest first: date separators, the "NEW" unread divider, the
	 * empty / loading lines, and the "Jump to present" pill when scrolled up. Each task is a TaskMessage.
	 */
	import { ArrowDown } from "@lucide/svelte";
	import type { Crewmate, CrewmateTask } from "#lib/crew.ts";
	import { groupTasks } from "#lib/crew/crewLogic.ts";
	import type { ChatState } from "./chat.svelte.ts";
	import TaskMessage from "./TaskMessage.svelte";
	import { fmtDate, needsDateSeparator } from "./taskView.ts";
	import { triggerIcon } from "#lib/crew/settings/triggerIcons.ts";
	import "./chat.css";

	interface Props {
		/** Tasks oldest first. */
		tasks: CrewmateTask[];
		loading: boolean;
		crewmate: Crewmate;
		hostKey: string;
		chat: ChatState;
		/** Index of the first task newer than the last time the user looked, or -1. */
		unreadIdx: number;
		now: number;
		onterminal: () => void;
	}

	let { tasks, loading, crewmate, hostKey, chat, unreadIdx, now, onterminal }: Props = $props();

	const grouped = $derived(groupTasks(tasks));
</script>

<div
	class="thread"
	bind:this={chat.threadEl}
	onscroll={() => chat.onThreadScroll()}
	aria-label="Task history"
>
	{#if loading && tasks.length === 0}
		<div class="loading">Loading…</div>
	{:else if tasks.length === 0}
		<div class="empty-thread">No tasks yet. Send one below.</div>
	{:else}
		{#each tasks as t, i (t.id)}
			{#if i === unreadIdx}
				<div class="unread-divider" role="separator">
					<span class="unread-label">NEW</span>
				</div>
			{/if}
			{#if needsDateSeparator(tasks, !!grouped[i]?.groupStart, i)}
				<div class="msg-ts">{fmtDate(t.ts)}</div>
			{/if}
			<TaskMessage
				task={t}
				{crewmate}
				{hostKey}
				{chat}
				{now}
				groupStart={!!grouped[i]?.groupStart}
				triggerIcon={t.trigger_event ? triggerIcon(t.trigger_event.kind) : undefined}
				{onterminal}
			/>
		{/each}
	{/if}
</div>

{#if chat.jumpVisible}
	<div class="jump-present" role="status">
		<button class="jump-btn" onclick={() => chat.jumpToBottom()}
			>Jump to present <ArrowDown size={12} aria-hidden="true" /></button
		>
	</div>
{/if}

<style>
	.empty-thread {
		color: var(--ca-text-secondary);
		font-size: 14px;
		text-align: center;
		padding: 40px 0;
	}
	.loading {
		color: var(--ca-text-secondary);
		font-size: 14px;
		padding: 12px 0;
	}
	.msg-ts {
		text-align: center;
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin: 12px 0;
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.msg-ts::before,
	.msg-ts::after {
		content: "";
		flex: 1;
		height: 0.5px;
		background: var(--ca-outline-subtle);
	}
	.unread-divider {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 12px 16px;
		color: var(--ca-semantic-error, #e03131);
	}
	.unread-divider::before,
	.unread-divider::after {
		content: "";
		flex: 1;
		height: 1px;
		background: var(--ca-semantic-error, #e03131);
		opacity: 0.5;
	}
	.unread-label {
		font-size: 10.5px;
		font-weight: 700;
		letter-spacing: 0.05em;
	}
	.jump-present {
		position: absolute;
		bottom: 130px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 20;
		pointer-events: none;
	}
	.jump-btn {
		pointer-events: all;
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		border: none;
		border-radius: 20px;
		padding: 5px 14px;
		font-size: 12px;
		cursor: default;
		box-shadow: 0 2px 8px rgb(0 0 0 / 0.3);
	}
</style>
