<script lang="ts">
	/**
	 * TaskActionCard — the card shown while a task is `waiting_action`: connect GitHub, provide a secret, answer a
	 * question, pick an option, sign in to the engine, or approve an automation. One card, one header and one
	 * Cancel path for every kind; the free-text kinds share ActionTextAnswer.
	 */
	import { Button } from "@codeatlasdev/ui";
	import type { TaskAction } from "#lib/crew.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import { describeTrigger } from "#lib/crew/automations.ts";
	import GithubDeviceFlow from "#lib/crew/GithubDeviceFlow.svelte";
	import ActionTextAnswer from "./ActionTextAnswer.svelte";
	import type { ChatState } from "./chat.svelte.ts";

	interface Props {
		action: TaskAction;
		taskId: string;
		crewmateId: string;
		crewmateName: string;
		hostKey: string;
		chat: ChatState;
	}

	let { action, taskId, crewmateId, crewmateName, hostKey, chat }: Props = $props();

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
		<div class="action-title">Connect GitHub to continue</div>
		<div class="action-reason">{action.reason}</div>
		{#if status?.login}
			<div class="action-connected">Connected as @{status.login}</div>
		{:else if flow}
			<GithubDeviceFlow {flow} {polling} oncancel={() => githubCrew.stopPolling(hostKey)} />
		{:else}
			<div class="action-btns">
				<Button size="sm" onclick={() => void githubCrew.startLogin(hostKey)}>Connect</Button>
				<Button variant="ghost" size="sm" onclick={cancel}>Cancel task</Button>
			</div>
		{/if}
	{:else if action.kind === "provide_secret"}
		<div class="action-title">{action.label}</div>
		<div class="action-reason">{action.reason}</div>
		<ActionTextAnswer
			type="password"
			placeholder="Enter value"
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
		<div class="action-title">Sign in to {action.engine}</div>
		<div class="action-reason">Sign in to {action.engine} in Settings, then click Done.</div>
		<div class="action-btns">
			<Button size="sm" disabled={submitting} onclick={() => answer("done")}>Done</Button>
			<Button variant="ghost" size="sm" onclick={cancel}>Cancel</Button>
		</div>
	{:else if action.kind === "add_trigger"}
		<div class="action-title">{crewmateName} wants to add an automation</div>
		<div class="action-reason">{action.reason}</div>
		<div class="action-detail">{describeTrigger(action.trigger)}</div>
		{#if action.prompt}
			<div class="action-reason">{action.prompt}</div>
		{/if}
		<div class="action-btns">
			<Button size="sm" disabled={submitting} onclick={() => answer("add")}>Add</Button>
			<Button variant="ghost" size="sm" disabled={submitting} onclick={() => answer("skip")}>Not now</Button>
		</div>
	{:else}
		<div class="action-title">{(action as { label?: string }).label ?? "Response needed"}</div>
		<ActionTextAnswer
			label="Response"
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
	.action-options {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
</style>
