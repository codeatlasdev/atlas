<script lang="ts">
	/**
	 * ApprovalPrompt — the inline "Wants to run <tool>" bubble under the running task while the crewmate waits for the
	 * user's permission: Allow once (primary), Allow for this task, Deny, and an optional note that reaches the agent.
	 * A call a rail held (a protected path, a task from GitHub or a webhook) says so and offers no task grant. Once
	 * answered it leaves one line saying what was answered. Keys, while focus is in the prompt: Esc denies,
	 * Cmd/Ctrl+Enter allows once (also from the note field).
	 */
	import { Check, X } from "@lucide/svelte";
	import { Button } from "@codeatlasdev/ui";
	import type { PendingApproval } from "#lib/crew.ts";
	import { embedIn, msgIn } from "#lib/crew/motion.ts";
	import {
		COMMENT_MAX,
		canGrantForTask,
		makeAnswer,
		railText,
		type ApprovalAnswer,
	} from "#lib/crew/permissionMode.ts";
	import { kbd, primaryKey } from "#lib/keys.ts";
	import { fmtTime, formatApprovalInput } from "./taskView.ts";
	import type { ChatState } from "./chat.svelte.ts";
	import VoiceMic from "./VoiceMic.svelte";
	import { voiceStatus } from "#lib/crew/voice/status.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		approval: PendingApproval;
		/** What was answered in this view, and when (ms); absent while the prompt is open. */
		answered?: ApprovalAnswer & { at: number };
		/** The crewmate's name, for the note's label. */
		name: string;
		onanswer: (paneKey: string, req: string, answer: ApprovalAnswer) => void;
		/** With the chat the prompt has a microphone: a spoken answer goes to hostd, which decides what it means. */
		chat?: ChatState;
		hostKey?: string;
	}

	let { approval: pa, answered, name, onanswer, chat, hostKey = "" }: Props = $props();

	let note = $state("");
	/**
	 * Only the transcript leaves (`agent_approve_voice`). A verdict that decided closes the prompt; a `comment`
	 * becomes the note for the person to confirm with a tap; anything else says why and decides nothing.
	 */
	// svelte-ignore state_referenced_locally (the chat of a prompt never changes; the prompt itself is read at each use)
	const voice = chat?.voiceAnswer(
		"approval",
		(transcript) => chat.approvalVoice(pa.pane_key, pa.req, transcript),
		(_, view) => {
			if (view.decided && "allow" in view.decided) chat.approvedByVoice(pa.req, view.decided.allow);
			else if (view.note) note = view.note.slice(0, COMMENT_MAX);
		},
	);
	const answer = (allow: boolean, scope: "once" | "task" = "once") =>
		onanswer(pa.pane_key, pa.req, makeAnswer(allow, scope, note));

	function onkeydown(e: KeyboardEvent) {
		if (e.defaultPrevented || e.isComposing) return;
		// While the microphone is on, Escape stops it (and forgets what was said); it never denies.
		if (e.key === "Escape" && voice?.busy) {
			e.preventDefault();
			voice.cancel();
			return;
		}
		if (e.key === "Enter" && primaryKey(e) && !e.altKey && !e.shiftKey) {
			e.preventDefault();
			answer(true);
		} else if (e.key === "Escape" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
			e.preventDefault();
			answer(false);
		}
	}

	const answeredText = $derived(
		answered
			? !answered.allow
				? m.crew_permission_answered_denied({ time: fmtTime(answered.at) })
				: answered.scope === "task"
					? m.crew_permission_answered_task({ time: fmtTime(answered.at) })
					: m.crew_permission_answered_once({ time: fmtTime(answered.at) })
			: "",
	);
</script>

{#if answered}
	<div class="approval-answered" in:msgIn>
		{#if answered.allow}<Check size={12} aria-hidden="true" />{:else}<X size={12} aria-hidden="true" />{/if}
		{answeredText}
	</div>
{:else}
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div class="approval-bubble" role="alert" in:embedIn {onkeydown}>
		<div class="approval-tool">
			{m.crew_chat_approval_prompt_wants_to_run()} <strong>{pa.tool_name}</strong>
		</div>
		<pre class="approval-input">{formatApprovalInput(pa.tool_name ?? "", pa.tool_input)}</pre>
		{#if pa.rail}
			<div class="approval-rail">{railText(pa.rail)}</div>
		{/if}
		<input
			class="approval-note"
			type="text"
			maxlength="2000"
			bind:value={note}
			aria-label={m.crew_permission_comment_label({ name })}
			placeholder={m.crew_permission_comment_placeholder()}
		/>
		{#if voice}
			<div class="approval-voice">
				<VoiceMic session={voice} unsupported={!!voiceStatus.loaded[hostKey] && !voiceStatus.of(hostKey)} />
			</div>
		{/if}
		<div class="approval-btns">
			<Button
				size="sm"
				onclick={() => answer(true)}
				title={m.crew_permission_allow_once_title({ key: kbd("cmd-enter") })}
				>{m.crew_permission_allow_once()}</Button
			>
			{#if canGrantForTask(pa)}
				<Button size="sm" variant="ghost" onclick={() => answer(true, "task")}
					>{m.crew_permission_allow_task()}</Button
				>
			{/if}
			<Button
				size="sm"
				variant="ghost"
				onclick={() => answer(false)}
				title={m.crew_permission_deny_title({ key: kbd("escape") })}>{m.crew_permission_deny()}</Button
			>
		</div>
	</div>
{/if}

<style>
	.approval-bubble {
		max-width: 72%;
		padding: 10px 14px;
		border-radius: 18px;
		border-bottom-left-radius: 4px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, var(--ca-surface-sunken));
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
		font-size: 13px;
	}
	.approval-tool {
		font-weight: 600;
		margin-bottom: 6px;
	}
	.approval-input {
		font-family: var(--ca-font-mono, monospace);
		font-size: 11px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
		border-radius: 6px;
		padding: 6px 8px;
		margin: 6px 0;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.approval-rail {
		font-size: 12px;
		color: color-mix(in oklab, var(--ca-semantic-warning) 80%, var(--ca-text-primary));
		margin: 4px 0;
	}
	.approval-note {
		width: 100%;
		box-sizing: border-box;
		margin-top: 6px;
		padding: 5px 8px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-surface-raised, var(--ca-surface-base)) 80%, transparent);
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12px;
	}
	.approval-note:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
		outline-offset: 1px;
	}
	.approval-voice {
		margin-top: 8px;
	}
	.approval-btns {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 8px;
	}
	.approval-answered {
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 4px;
		margin-top: 2px;
	}
</style>
