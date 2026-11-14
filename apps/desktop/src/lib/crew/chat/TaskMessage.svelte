<script lang="ts">
	/**
	 * TaskMessage — one task in the thread: the trigger line, your prompt, a mid-task answer pair, then what the
	 * crewmate says for the task's state (running / done / failed / cancelled / waiting for an action), and the
	 * hover toolbar. Queued tasks show only the prompt and a "Queued" line.
	 */
	import { RotateCcw, Terminal, X } from "@lucide/svelte";
	import { CopyButton } from "@codeatlasdev/ui";
	import type { Component } from "svelte";
	import type { Crewmate, CrewmateTask } from "#lib/crew.ts";
	import CrewTyping from "#lib/crew/CrewTyping.svelte";
	import type { CrewmateLook } from "#lib/crew/look.ts";
	import { msgIn, msgOut } from "#lib/crew/motion.ts";
	import { workingLabel } from "#lib/crew/stepPhrase.ts";
	import TaskWork from "#lib/crew/TaskWork.svelte";
	import ApprovalPrompt from "./ApprovalPrompt.svelte";
	import type { ChatState } from "./chat.svelte.ts";
	import TaskActionCard from "./TaskActionCard.svelte";
	import TaskReply from "./TaskReply.svelte";
	import { fmtElapsed } from "./taskView.ts";

	interface Props {
		task: CrewmateTask;
		crewmate: Crewmate;
		hostKey: string;
		chat: ChatState;
		groupStart: boolean;
		now: number;
		/** Icon for the trigger line, resolved by the thread. */
		triggerIcon?: Component;
		onterminal: () => void;
	}

	let {
		task: t,
		crewmate,
		hostKey,
		chat,
		groupStart,
		now,
		triggerIcon: TrigIcon,
		onterminal,
	}: Props = $props();

	const look = $derived(crewmate.look as CrewmateLook | undefined);
	const approval = $derived(crewmate.running_task?.id === t.id ? crewmate.pending_approval : undefined);
</script>

<div class="msg-row" data-task-id={t.id} class:flash={chat.flashTask === t.id} class:group-start={groupStart}>
	{#if t.trigger_event && TrigIcon}
		<div class="trigger-event" role="note">
			<span class="trigger-icon" aria-hidden="true"><TrigIcon size={14} /></span>
			<span class="trigger-summary">{t.trigger_event.summary}</span>
		</div>
	{/if}
	<div class="msg-out" in:msgOut>
		<div class="bubble out ca-selectable">{t.prompt}</div>
		{#if t.source !== "desktop"}
			<div class="msg-meta">· {t.source}</div>
		{/if}
		{#if t.preparing}
			<!-- A first task on a big repo waited over a minute as a bare "Queued". -->
			<div class="msg-meta muted preparing">
				<span class="prep-dot" aria-hidden="true"></span>Downloading {t.preparing}…
			</div>
		{:else if t.state === "queued"}
			<div class="msg-meta muted">Queued</div>
		{/if}
	</div>

	<!-- A choice/answer the user gave mid-task stays in the conversation: the crewmate's question, then the user's
	     pick as their own message. -->
	{#if t.answer && t.state !== "waiting_action" && t.action?.kind !== "add_trigger"}
		{#if t.action?.kind === "choose" || t.action?.kind === "input"}
			<div class="msg-in" in:msgIn>
				<div class="msg-in-body">
					<div class="bubble in">{t.action.kind === "choose" ? t.action.question : t.action.label}</div>
				</div>
			</div>
		{/if}
		<div class="msg-out" in:msgOut>
			<div class="bubble out ca-selectable">{t.answer}</div>
		</div>
	{/if}

	{#if t.state === "running"}
		<div class="msg-in" class:group-start={groupStart} in:msgIn>
			<div class="msg-in-body">
				<!-- Same "dots gather, crewmate hops" as the onboarding. The line says what it is doing: its latest
				     step while fresh, then "Thinking", "Planning"... changing every few seconds. -->
				<CrewTyping
					seed={crewmate.id}
					{look}
					label={workingLabel(t.steps?.at(-1)?.text, t.steps?.at(-1)?.ts, now)}
					detail={fmtElapsed(t.started_at ? now - t.started_at : 0)}
				/>
				<!-- While it works, what it is doing stays on screen: a section per title the agent set itself, its
				     steps ticking off, finished sections folded. -->
				<TaskWork steps={t.steps ?? []} title={t.title} running />
				{#if approval}
					<ApprovalPrompt
						{approval}
						answered={chat.approvedReqs.has(approval.req)}
						onanswer={(pane, req, allow) => void chat.approval(pane, req, allow)}
					/>
				{/if}
			</div>
		</div>
	{:else if t.state === "done"}
		<div class="msg-in" class:group-start={groupStart} in:msgIn>
			<div class="msg-in-body">
				<!-- The work before the reply, folded per section; it stays in the conversation. -->
				<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
				<TaskReply task={t} {chat} />
			</div>
		</div>
	{:else if t.state === "failed"}
		<div class="msg-in" class:group-start={groupStart} in:msgIn>
			<div class="msg-in-body">
				<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
				<div class="bubble in error ca-selectable">{t.error ?? "Failed"}</div>
			</div>
		</div>
	{:else if t.state === "cancelled"}
		<div class="msg-in" class:group-start={groupStart} in:msgIn>
			<div class="msg-in-body">
				<TaskWork steps={t.steps ?? []} title={t.title} running={false} />
				<div class="task-note"><X size={12} aria-hidden="true" /> Cancelled</div>
			</div>
		</div>
	{:else if t.state === "waiting_action" && t.action}
		<div class="msg-in" class:group-start={groupStart} in:msgIn>
			<div class="msg-in-body">
				<TaskActionCard
					action={t.action}
					taskId={t.id}
					crewmateId={crewmate.id}
					crewmateName={crewmate.name}
					{hostKey}
					{chat}
				/>
			</div>
		</div>
	{/if}

	<div class="msg-toolbar">
		<CopyButton value={t.state === "done" ? (t.result ?? "") : t.prompt} label="" size="sm" />
		{#if t.state === "failed" || t.state === "cancelled"}
			<button class="tb-btn" title="Retry" onclick={() => void chat.sendText(t.prompt)}>
				<RotateCcw size={14} />
			</button>
		{/if}
		{#if t.state === "running" && crewmate.running_task?.pane}
			<button class="tb-btn" title="Open terminal" onclick={onterminal}><Terminal size={14} /></button>
		{/if}
	</div>
</div>

<style>
	/* msg-row wraps user + crewmate bubbles for a single task */
	.msg-row {
		position: relative;
		display: flex;
		flex-direction: column;
	}
	.msg-row.flash {
		animation: task-flash 1.6s ease-out;
		border-radius: 12px;
	}
	@keyframes task-flash {
		0%,
		30% {
			background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
		}
		100% {
			background: transparent;
		}
	}
	.msg-meta {
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin-top: 2px;
		padding: 0 4px;
	}
	.msg-meta.preparing {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.prep-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-brand-main);
		animation: prep-pulse 1.2s ease-in-out infinite;
	}
	@keyframes prep-pulse {
		50% {
			opacity: 0.3;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.prep-dot {
			animation: none;
		}
	}
	.trigger-event {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 16px 3px 28px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		margin-top: 8px;
	}
	.trigger-icon {
		font-size: 12px;
		flex: none;
	}
	.trigger-summary {
		font-style: italic;
	}

	/* ─── hover toolbar ──────────────────────────────────────── */
	.msg-toolbar {
		position: absolute;
		top: -10px;
		right: 8px;
		display: none;
		background: var(--ca-surface-overlay, #2a2a2a);
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 2px 4px;
		gap: 2px;
		flex-direction: row;
		align-items: center;
		z-index: 10;
		box-shadow: 0 2px 8px rgb(0 0 0 / 0.2);
	}
	.msg-row:hover .msg-toolbar {
		display: flex;
	}
	.tb-btn {
		width: 26px;
		height: 26px;
		border: none;
		background: none;
		border-radius: 5px;
		color: var(--ca-text-secondary);
		cursor: default;
		font-size: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.tb-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
</style>
