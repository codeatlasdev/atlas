<script lang="ts">
	/**
	 * ChatHeader — who you are talking to and what it is doing (clicking opens its settings), plus the Terminal /
	 * Stop buttons while a task runs, and the offline banner of its crew server.
	 */
	import { ChevronDown, Terminal, TriangleAlert, X } from "@lucide/svelte";
	import type { Crewmate } from "#lib/crew.ts";
	import CrewmateFace from "#lib/crew/CrewmateFace.svelte";
	import type { CrewmateState as FaceState } from "#lib/crew/crewmateFace.ts";
	import ModeBadge from "./ModeBadge.svelte";
	import { modeOf } from "#lib/crew/permissionMode.ts";
	import { fmtElapsed } from "./taskView.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		crewmate: Crewmate;
		/** Label of the engine shown while idle ("Claude Code"). */
		agentLabel: string;
		/** `started_at` (ms) of the running task, when it has one. */
		runningStart?: number;
		/** Ticking clock (ms) so the elapsed time moves. */
		now: number;
		hostOnline: boolean;
		serverName: string;
		onsettings: () => void;
		onterminal: () => void;
		onstop: () => void;
	}

	let {
		crewmate,
		agentLabel,
		runningStart,
		now,
		hostOnline,
		serverName,
		onsettings,
		onterminal,
		onstop,
	}: Props = $props();

	const faceState = $derived<FaceState>(
		crewmate.state === "needs_you" ? "needs_you" : crewmate.state === "working" ? "working" : "idle",
	);
</script>

<div class="header">
	<button
		class="header-pill"
		onclick={onsettings}
		aria-label={m.crew_chat_header_crewmate_settings_name({ name: crewmate.name })}
	>
		<CrewmateFace seed={crewmate.id} look={crewmate.look} state={faceState} size={30} />
		<span class="pill-text">
			<span class="pill-name">{crewmate.name}</span>
			<span class="pill-status" data-state={crewmate.state}>
				<span class="pill-dot" aria-hidden="true"></span>
				{#if crewmate.state === "working"}
					<!-- Only with a real start: a task waiting for the user has none, and `now - 0` showed the minutes
					     since 1970 ("29886587m"). -->
					{runningStart
						? m.crew_chat_header_working_elapsed({ elapsed: fmtElapsed(now - runningStart) })
						: m.crew_chat_header_working()}
				{:else if crewmate.state === "needs_you"}
					{m.crew_chat_header_needs_you()}
				{:else}
					{agentLabel}
				{/if}
				<ModeBadge mode={modeOf(crewmate)} size={11} />
			</span>
		</span>
		<ChevronDown size={14} class="pill-chevron" aria-hidden="true" />
	</button>
	<!-- Shown only while there is a task to watch or stop. -->
	{#if crewmate.running_task}
		<div class="header-right">
			{#if crewmate.running_task.pane}
				<button class="head-btn" onclick={onterminal} title={m.crew_chat_header_watch_in_a_terminal()}>
					<Terminal size={14} aria-hidden="true" />
					{m.crew_chat_header_terminal()}
				</button>
			{/if}
			<button class="head-btn danger" onclick={onstop} title={m.crew_chat_header_stop_the_current_task()}>
				<X size={14} aria-hidden="true" />
				{m.crew_chat_header_stop()}
			</button>
		</div>
	{/if}
</div>

{#if !hostOnline}
	<div class="offline-banner" role="alert">
		<TriangleAlert size={14} aria-hidden="true" />
		{serverName
			? m.crew_chat_header_server_offline({ server: serverName })
			: m.crew_chat_header_server_offline_unnamed()}
	</div>
{/if}

<style>
	.header {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 12px;
		height: 44px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		position: relative;
		flex-shrink: 0;
	}
	.header-pill {
		display: flex;
		align-items: center;
		gap: 10px;
		background: none;
		border: none;
		border-radius: 12px;
		padding: 4px 10px 4px 4px;
		color: var(--ca-text-primary);
		cursor: default;
		transition: background 120ms;
		text-align: left;
	}
	.header-pill:hover,
	.header-pill:focus-visible {
		background: var(--ca-vibrancy-fill);
		outline: none;
	}
	.pill-text {
		display: flex;
		flex-direction: column;
		line-height: 1.2;
		min-width: 0;
	}
	.pill-name {
		font-size: 14px;
		font-weight: 600;
	}
	.pill-status {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.pill-status :global(.mode-badge) {
		margin-left: 6px;
		padding-left: 8px;
		border-left: 0.5px solid var(--ca-outline-subtle);
	}
	.pill-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-tertiary, var(--ca-text-secondary));
		opacity: 0.6;
	}
	.pill-status[data-state="working"] .pill-dot {
		background: var(--ca-semantic-success);
		opacity: 1;
		animation: pill-pulse 1.6s ease-in-out infinite;
	}
	.pill-status[data-state="needs_you"] {
		color: var(--ca-semantic-warning);
	}
	.pill-status[data-state="needs_you"] .pill-dot {
		background: var(--ca-semantic-warning);
		opacity: 1;
	}
	@keyframes pill-pulse {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.pill-status[data-state="working"] .pill-dot {
			animation: none;
		}
	}
	.header-pill :global(.pill-chevron) {
		color: var(--ca-text-secondary);
		opacity: 0;
		transition: opacity 120ms;
	}
	.header-pill:hover :global(.pill-chevron),
	.header-pill:focus-visible :global(.pill-chevron) {
		opacity: 1;
	}
	.header-right {
		position: absolute;
		right: 12px;
		top: 50%;
		transform: translateY(-50%);
		display: flex;
		gap: 6px;
	}
	.head-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font: inherit;
		font-size: 12px;
		padding: 5px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.head-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.head-btn.danger:hover {
		color: var(--ca-semantic-error);
		border-color: color-mix(in oklab, var(--ca-semantic-error) 40%, transparent);
	}
	.offline-banner {
		flex: none;
		padding: 6px 16px;
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 12%, var(--ca-surface-base));
		color: color-mix(in oklab, var(--ca-semantic-warning) 80%, var(--ca-text-primary));
		border-bottom: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 25%, transparent);
	}
</style>
