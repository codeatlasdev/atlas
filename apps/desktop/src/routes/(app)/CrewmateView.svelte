<script lang="ts">
	/**
	 * CrewmateView: the chat with the selected crewmate. Header, then either the setup conversation (a new
	 * crewmate) or the task thread and composer. The parts live in lib/crew/chat; this wires them to the stores.
	 * Approval is inline in the thread; Telegram and Pair phone live in the settings dialog.
	 */
	import { onMount } from "svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { normalizeLook } from "#lib/crew/look.ts";
	import { firstUnreadIndex } from "#lib/crew/crewLogic.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import type { SetupStep } from "#lib/crew/crewSetup.ts";
	import CrewmateSettingsDialog from "#lib/crew/settings/CrewmateSettingsDialog.svelte";
	import ChatHeader from "#lib/crew/chat/ChatHeader.svelte";
	import ChatThread from "#lib/crew/chat/ChatThread.svelte";
	import Composer from "#lib/crew/chat/Composer.svelte";
	import SetupConversation from "#lib/crew/chat/SetupConversation.svelte";
	import { ChatState } from "#lib/crew/chat/chat.svelte.ts";
	import { parseAsk } from "#lib/crew/chat/parseAsk.ts";

	const chat = new ChatState();

	let settingsOpen = $state(false);
	let now = $state(Date.now());
	/** Actual hostname of the local hostd, fetched on first selection of a local crewmate. */
	let localHostname = $state("");

	const crewmate = $derived(crew.selectedCrewmate);
	const tasks = $derived(crew.tasks);
	const hostKey = $derived(crew.selected?.hostKey ?? "");
	const hostOnline = $derived(crew.selected ? crew.serverOk(crew.selected.hostKey) : true);
	const serverName = $derived(crew.selected?.hostName ?? "");
	/** Show actual hostname for local; server name label for remote. */
	const displayServerName = $derived(hostKey ? serverName : localHostname || serverName);
	const queuedCount = $derived(tasks.filter((t) => t.state === "queued").length);
	/** True when the most recent task has an unanswered ask card. */
	const hasPendingAsk = $derived(
		tasks.length > 0 && parseAsk(tasks[0].result) !== null && !chat.approvedReqs.has(tasks[0].id),
	);
	const orderedTasks = $derived([...tasks].reverse()); // oldest first
	const unreadIdx = $derived(firstUnreadIndex(orderedTasks, crew.selectedLastSeen));

	const AGENT_LABELS: Record<string, string> = {
		omp: "omp",
		claude: "Claude Code",
		codex: "Codex",
		opencode: "OpenCode",
	};
	const agentLabel = $derived(
		crewmate
			? (agents.list.find((a) => a.id === crewmate.agent)?.label ??
					AGENT_LABELS[crewmate.agent] ??
					crewmate.agent)
			: "",
	);
	/** Only with a real start: a task waiting for the user has none. */
	const runningStart = $derived(
		crewmate?.running_task ? tasks.find((t) => t.id === crewmate.running_task!.id)?.started_at : undefined,
	);

	/** IDE projects for the setup conversation's project question. */
	const ideProjects = $derived(
		ws.doc.worktrees.map((w) => ({ id: w.id, name: w.name, path: w.path, branch: w.branch ?? "" })),
	);
	const setupStep = $derived(crewmate?.setup_step as SetupStep | undefined);

	// Reset approvedReqs when the selected crewmate changes
	$effect(() => {
		void crew.selected;
		chat.resetApprovals();
	});

	// Fetch actual hostname for local host so header shows e.g. "Mac" not "This computer"
	$effect(() => {
		if (crew.selected && !crew.selected.hostKey) {
			hostCall<{ hostname: string }>("info")
				.then((r) => {
					localHostname = r.hostname;
				})
				.catch(() => {});
		}
	});

	onMount(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// Auto-scroll to bottom when new tasks arrive, if already at bottom
	$effect(() => {
		void orderedTasks.length;
		chat.followBottom();
	});

	// Opened from a Board card: scroll to that task once it is rendered, and flash it.
	$effect(() => {
		void crew.focusTask;
		void crew.tasksLoading;
		chat.focusRequestedTask(orderedTasks.map((t) => t.id));
	});

	// Auto-resume: when GitHub connects while a waiting_action connect_github task is parked
	$effect(() => {
		if (!hostKey) return;
		if (!githubCrew.statuses.get(hostKey)?.login) return;
		if (tasks.some((t) => t.state === "waiting_action" && t.action?.kind === "connect_github")) {
			void hostCall("crew_github_connect_resume", { host: hostKey });
		}
	});

	async function openTerminal() {
		if (!crewmate?.running_task?.pane || !crew.selected) return;
		const pty = crewmate.running_task.pane;
		const args = hostKey ? { pty, host: hostKey } : { pty };
		await hostCall("pty_attach", { ...args, replay: true, output: true }).catch((e) =>
			toast.error(errText(e)),
		);
	}

	async function cancelRunningTask() {
		if (!crewmate?.running_task) return;
		await crew.cancelTask(crewmate.running_task.id).catch((e) => toast.error(errText(e)));
	}
</script>

<div class="crewmate-view">
	{#if !crewmate}
		<div class="empty">Select a crewmate from the sidebar.</div>
	{:else}
		<ChatHeader
			{crewmate}
			{agentLabel}
			{runningStart}
			{now}
			{hostOnline}
			serverName={displayServerName}
			onsettings={() => (settingsOpen = true)}
			onterminal={() => void openTerminal()}
			onstop={() => void cancelRunningTask()}
		/>

		<!-- --crew: this crewmate's own colour, so your bubbles and the composer match the crewmate you're talking to. -->
		<div class="chat-col" style:--crew={normalizeLook(crewmate.look, crewmate.id).color}>
			{#if setupStep}
				<SetupConversation {crewmate} step={setupStep} {ideProjects} />
			{:else}
				<ChatThread
					tasks={orderedTasks}
					loading={crew.tasksLoading}
					{crewmate}
					{hostKey}
					{chat}
					{unreadIdx}
					{now}
					onterminal={() => void openTerminal()}
				/>
				<Composer
					bind:value={chat.prompt}
					placeholder={hasPendingAsk
						? "Type your answer or talk about something else"
						: `Message ${crewmate.name ?? "crewmate"}…`}
					label="Task prompt"
					busy={chat.sending}
					onattach={() => {}}
					queued={queuedCount}
					onsend={() => void chat.send()}
				/>
			{/if}
		</div>

		<CrewmateSettingsDialog bind:open={settingsOpen} />
	{/if}
</div>

<style>
	.crewmate-view {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		overflow: hidden;
		background: var(--ca-surface-base);
		position: relative;
	}

	.empty {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--ca-text-secondary);
		font-size: 14px;
	}

	.chat-col {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		/* Like Grok: the thread uses the whole pane on a normal window and only centres (capped) once the pane
		   is much wider than a comfortable reading line, i.e. full screen. */
		max-width: 1120px;
		width: 100%;
		margin: 0 auto;
		position: relative;
	}
</style>
