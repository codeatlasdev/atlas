<script lang="ts">
	/**
	 * TaskActionCard — the card shown while a task is `waiting_action`: connect GitHub, provide a secret, answer a
	 * question, pick an option, sign in to the engine, approve an automation, or decide on work Atlas's guards refused. One card, one header and one
	 * Cancel path for every kind; the free-text kinds share ActionTextAnswer.
	 */
	import { Button } from "@codeatlasdev/ui";
	import type { TaskAction } from "#lib/crew.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import { describeTrigger } from "#lib/crew/automations.ts";
	import GithubDeviceFlow from "#lib/crew/GithubDeviceFlow.svelte";
	import ActionTextAnswer from "./ActionTextAnswer.svelte";
	import type { ChatState } from "./chat.svelte.ts";
	import type { Attachment } from "#lib/crew/attachments.ts";
	import ActionImages from "./ActionImages.svelte";
	import TaskImages from "./TaskImages.svelte";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		/** The task's images (`Task.attachments`); the card shows the agent's. */
		attachments?: Attachment[];
		action: TaskAction;
		taskId: string;
		crewmateId: string;
		crewmateName: string;
		hostKey: string;
		chat: ChatState;
	}

	let { action, taskId, crewmateId, crewmateName, hostKey, attachments = [], chat }: Props = $props();

	/** What the card is about: the agent's own captures (a user's image is under their prompt, not here). */
	const agentImages = $derived(attachments.filter((a) => a.source === "agent"));
	/** The sentences under the title, for the two kinds that can be retried. */
	const reasons = $derived(
		action.kind === "refused" ? action.reasons : action.kind === "prove_ui" ? [action.reason] : [],
	);

	const flow = $derived(githubCrew.flows.get(hostKey));
	const polling = $derived(githubCrew.isPolling(hostKey));
	const status = $derived(githubCrew.statuses.get(hostKey));
	const submitting = $derived(chat.actionSubmitting.has(taskId));

	const answer = (value: string) => void chat.answerAction(taskId, crewmateId, value);
	const cancel = () => void chat.cancelAction(taskId, crewmateId);

	function setInput(value: string) {
		chat.actionInputs = { ...chat.actionInputs, [taskId]: value };
	}
	function setSecret(value: string) {
		chat.actionSecrets = { ...chat.actionSecrets, [taskId]: value };
	}
</script>

<div class="action-card" role="alert">
	{#if action.kind === "connect_github"}
		<div class="action-title">{m.crew_chat_task_action_card_connect_github_to_continue()}</div>
		<div class="action-reason">{action.reason}</div>
		{#if status?.login}
			<div class="action-connected">
				{m.crew_chat_task_action_card_connected_as_login({ account: `@${status.login}` })}
			</div>
		{:else if flow}
			<GithubDeviceFlow {flow} {polling} oncancel={() => githubCrew.stopPolling(hostKey)} />
		{:else}
			<div class="action-btns">
				<Button size="sm" onclick={() => void githubCrew.startLogin(hostKey)}
					>{m.crew_chat_task_action_card_connect()}</Button
				>
				<Button variant="ghost" size="sm" onclick={cancel}
					>{m.crew_chat_task_action_card_cancel_task()}</Button
				>
			</div>
		{/if}
	{:else if action.kind === "provide_secret"}
		<div class="action-title">{action.label}</div>
		<div class="action-reason">{action.reason}</div>
		<ActionTextAnswer
			type="password"
			placeholder={m.crew_chat_task_action_card_enter_value()}
			label={action.label}
			value={chat.actionSecrets[taskId] ?? ""}
			{submitting}
			onchange={setSecret}
			onsubmit={answer}
			oncancel={cancel}
		/>
	{:else if action.kind === "input"}
		<div class="action-title">{action.label}</div>
		{#if action.reason}
			<div class="action-reason">{action.reason}</div>
		{/if}
		<ActionTextAnswer
			placeholder={action.placeholder ?? ""}
			label={action.label}
			submitOnEnter
			value={chat.actionInputs[taskId] ?? ""}
			{submitting}
			onchange={setInput}
			onsubmit={answer}
			oncancel={cancel}
		/>
	{:else if action.kind === "choose"}
		<div class="action-title">{action.question}</div>
		<div class="action-options">
			{#each action.options as opt, oi (opt)}
				<button class="ask-option" disabled={submitting} onclick={() => answer(opt)}>
					<span class="ask-key">{String.fromCharCode(65 + oi)}</span>
					<div class="ask-opt-body">
						<span class="ask-opt-title">{opt}</span>
					</div>
				</button>
			{/each}
		</div>
	{:else if action.kind === "sign_in_engine"}
		<div class="action-title">
			{m.crew_chat_task_action_card_sign_in_to_engine({ engine: action.engine })}
		</div>
		<div class="action-reason">{m.crew_chat_task_action_card_sign_in_hint({ engine: action.engine })}</div>
		<div class="action-btns">
			<Button size="sm" disabled={submitting} onclick={() => answer("done")}>{m.common_action_done()}</Button>
			<Button variant="ghost" size="sm" onclick={cancel}>{m.common_action_cancel()}</Button>
		</div>
	{:else if action.kind === "add_trigger"}
		<div class="action-title">{m.crew_chat_task_action_card_wants_to_add_automation({ crewmateName })}</div>
		<div class="action-reason">{action.reason}</div>
		<div class="action-detail">{describeTrigger(action.trigger)}</div>
		{#if action.prompt}
			<div class="action-reason">{action.prompt}</div>
		{/if}
		<div class="action-btns">
			<Button size="sm" disabled={submitting} onclick={() => answer("add")}>{m.common_action_add()}</Button>
			<Button variant="ghost" size="sm" disabled={submitting} onclick={() => answer("skip")}
				>{m.crew_chat_task_action_card_not_now()}</Button
			>
		</div>
	{:else if action.kind === "refused" || action.kind === "prove_ui"}
		<div class="action-title">
			{action.kind === "prove_ui"
				? m.crew_chat_task_action_card_prove_ui_failed({ crewmateName })
				: m.crew_chat_task_action_card_work_refused({ crewmateName })}
		</div>
		{#if agentImages.length}
			<TaskImages attachments={agentImages} {taskId} {crewmateId} {hostKey} align="start" />
		{/if}
		<ul class="action-reasons">
			{#each reasons as reason (reason)}
				<li>{reason}</li>
			{/each}
		</ul>
		<!-- The marked screenshot goes with a retry, never with an override (hostd refuses it there). -->
		<ActionImages
			images={chat.actionImages[taskId] ?? []}
			disabled={submitting}
			onfiles={(files) => void chat.attachToAction(taskId, files)}
			onremove={(i) => chat.removeActionImage(taskId, i)}
		/>
		<div class="action-btns">
			<Button size="sm" disabled={submitting} onclick={() => answer("retry")}
				>{m.crew_chat_task_action_card_retry_with_these_reasons()}</Button
			>
			<Button variant="ghost" size="sm" disabled={submitting} onclick={() => answer("override")}
				>{m.crew_chat_task_action_card_override()}</Button
			>
		</div>
	{:else}
		<div class="action-title">
			{(action as { label?: string }).label ?? m.crew_chat_task_action_card_response_needed()}
		</div>
		<ActionTextAnswer
			label={m.crew_chat_task_action_card_response()}
			value={chat.actionInputs[taskId] ?? ""}
			{submitting}
			onchange={setInput}
			onsubmit={answer}
			oncancel={cancel}
		/>
	{/if}
</div>

<style>
	/* A comfortable fixed width instead of shrink-to-fit: the card used to collapse to its title's width and the
	   "Cancel task" button spilled out of it. */
	.action-card {
		box-sizing: border-box;
		width: min(380px, 100%);
		max-width: 100%;
		padding: 14px 16px;
		border-radius: 18px;
		border-top-left-radius: 4px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, transparent);
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 35%, transparent);
		display: flex;
		flex-direction: column;
		gap: 10px;
		overflow-wrap: anywhere;
	}
	.action-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.action-reason {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.action-connected {
		font-size: 12px;
		color: var(--ca-semantic-success);
	}
	.action-btns {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.action-btns :global(button) {
		white-space: nowrap;
	}
	.action-reasons {
		margin: 0;
		padding-left: 18px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.action-options {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
</style>
