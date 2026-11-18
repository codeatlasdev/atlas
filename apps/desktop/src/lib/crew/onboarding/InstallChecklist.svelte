<!--
  The install progress rail under the connect scene: every phase as a compact pill (done ✓ in the success colour, the
  active one in the accent with a pulse, pending muted, the failed one in the error colour) and, below, one line saying
  what the active (or failed) phase does. Same phases and order as the scene lights.
-->
<script lang="ts">
	import { Check } from "@lucide/svelte";
	import { installSteps, type InstallPhase } from "#lib/crew/installSteps.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		done,
		active,
		failed,
		browsers = false,
	}: {
		done: Set<InstallPhase>;
		active: InstallPhase | null;
		failed: InstallPhase | null;
		/** The install also sets up the capture browsers: show that step. */
		browsers?: boolean;
	} = $props();

	const steps = $derived(installSteps(browsers));
	const now = $derived(steps.find((s) => s.phase === (failed ?? active)));
</script>

<div class="install-checklist" data-testid="install-rail">
	<ol class="stepper" aria-label={m.crew_onboarding_install_checklist_install_progress()}>
		{#each steps as s, i (s.phase)}
			{@const isDone = done.has(s.phase)}
			{@const isActive = active === s.phase && failed !== s.phase}
			{@const isFailed = failed === s.phase}
			<li
				class="checklist-row"
				class:done={isDone}
				class:active={isActive}
				class:failed={isFailed}
				aria-current={isActive ? "step" : undefined}
				title={s.label}
			>
				<span class="dot" aria-hidden="true">
					{#if isDone}<Check size={11} />{:else if isFailed}!{:else}{i + 1}{/if}
				</span>
				<span class="sr-only">{s.label}</span>
			</li>
		{/each}
	</ol>
	<p class="now">
		{#if now}
			<b class:bad={failed !== null}>{now.label}</b>
			<span>{now.detail}</span>
		{:else}
			<b>{m.crew_onboarding_install_checklist_idle_title()}</b>
			<span>{m.crew_onboarding_install_checklist_idle_detail()}</span>
		{/if}
	</p>
</div>

<style>
	.install-checklist {
		display: flex;
		flex-direction: column;
		gap: 10px;
		width: 100%;
	}

	/* One line, in the real install order: a dot per phase, joined by a rail. */
	.stepper {
		display: flex;
		align-items: center;
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.checklist-row {
		display: flex;
		align-items: center;
		flex: 1;
		color: var(--ca-text-secondary);
	}

	.checklist-row:last-child {
		flex: 0;
	}

	.checklist-row::after {
		content: "";
		flex: 1;
		height: 2px;
		margin: 0 6px;
		background: var(--ob-line-idle, var(--ca-border-subtle));
		transition: background 300ms;
	}

	.checklist-row:last-child::after {
		display: none;
	}

	.checklist-row.done::after {
		background: var(--ob-ok, var(--ca-semantic-success));
	}

	.dot {
		flex-shrink: 0;
		width: 24px;
		height: 24px;
		box-sizing: border-box;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 11px;
		font-weight: 600;
		border: 2px solid var(--ob-line-idle, var(--ca-border-subtle));
		background: var(--ca-surface-base);
		transition:
			background 300ms,
			border-color 300ms,
			color 300ms;
	}

	.done .dot {
		background: var(--ob-ok, var(--ca-semantic-success));
		border-color: var(--ob-ok, var(--ca-semantic-success));
		color: var(--ca-surface-base);
	}

	.active .dot {
		background: color-mix(in srgb, var(--ob-accent, var(--ca-accent)) 18%, var(--ca-surface-base));
		border-color: var(--ob-accent, var(--ca-accent));
		color: var(--ob-accent, var(--ca-accent));
		animation: ob-beat 1100ms ease-in-out infinite;
	}

	.failed .dot {
		background: var(--ob-danger, var(--ca-danger));
		border-color: var(--ob-danger, var(--ca-danger));
		color: var(--ca-surface-base);
	}

	@keyframes ob-beat {
		50% {
			transform: scale(1.18);
		}
	}

	.now {
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin: 0;
		min-height: 2.9em;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}

	.now b {
		font-size: 14px;
		color: var(--ob-accent, var(--ca-accent));
	}

	.now b.bad {
		color: var(--ob-danger, var(--ca-danger));
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
