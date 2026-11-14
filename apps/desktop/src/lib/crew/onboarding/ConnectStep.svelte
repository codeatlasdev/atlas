<!-- Connect step form: SSH key check, address + port, the password flows and the "prefer the terminal" disclosure. -->
<script lang="ts">
	import { Check } from "@lucide/svelte";
	import { CopyButton } from "@codeatlasdev/ui";
	import CrewFlag from "#lib/crew/CrewFlag.svelte";
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();

	const shownTarget = $derived(ob.sshTarget.trim() || "user@host");
	const locked = $derived(ob.installRunning || ob.installDone || ob.authRunning);
</script>

<div class="form-block" style:--stagger={3}>
	{#if !ob.keyChecked}
		<div class="key-checking">
			<span class="spin-dot" aria-hidden="true"></span>
			<span>Checking SSH key…</span>
		</div>
	{:else if !ob.hasKey}
		<!-- No SSH key found -->
		<p class="key-missing-text">
			Atlas signs in to your server with an SSH key, like a password that never leaves your Mac. You don't
			have one yet.
		</p>
		{#if ob.keyError}
			<p class="field-error" role="alert">{ob.keyError}</p>
		{/if}
	{:else}
		<!-- Has SSH key: show address field -->
		<label class="field-label" for="ssh-target">SSH address</label>
		<div class="field-wrap" class:shake={ob.fieldShake}>
			<input
				id="ssh-target"
				class="ssh-input"
				bind:value={ob.sshTarget}
				placeholder="ubuntu@my-server.com"
				aria-label="SSH address"
				disabled={locked}
				onkeydown={(e) => e.key === "Enter" && ob.launchServer()}
			/>
		</div>
		<p class="field-hint">
			Uses your Mac's SSH key, the same way <code>ssh {shownTarget}</code> does.
		</p>

		<div class="port-row">
			<label class="field-label" for="ssh-port">Port</label>
			<input
				id="ssh-port"
				class="port-input"
				type="number"
				min={1}
				max={65535}
				bind:value={ob.sshPort}
				disabled={locked}
			/>
		</div>

		{#if ob.authMode === "authorize"}
			<!-- Permission denied: offer to copy key via password -->
			<div class="auth-block">
				<p class="auth-desc">
					Your Mac's SSH key isn't authorized on this server yet. Enter the server user's password to add it.
				</p>
				<input
					type="password"
					class="ssh-input"
					bind:value={ob.authPassword}
					placeholder="Server password"
					aria-label="Server password for SSH key authorization"
					disabled={ob.authRunning}
					onkeydown={(e) => e.key === "Enter" && ob.authorizeAndRetry()}
				/>
				{#if ob.authError}
					<p class="field-error" role="alert">{ob.authError}</p>
				{/if}
			</div>
		{:else if ob.authMode === "sudo"}
			<!-- sudo needs password -->
			<div class="auth-block">
				<p class="auth-desc">This user's sudo needs a password. Enter it to continue the install.</p>
				<input
					type="password"
					class="ssh-input"
					bind:value={ob.authPassword}
					placeholder="Sudo password"
					aria-label="Sudo password"
					disabled={ob.authRunning}
					onkeydown={(e) => e.key === "Enter" && ob.launchServer(ob.authPassword)}
				/>
				{#if ob.authError}
					<p class="field-error" role="alert">{ob.authError}</p>
				{/if}
			</div>
		{:else if ob.lastErrLine}
			<!-- Any failure, including one before the first step marker (e.g. the CLI can't run): never shake silently. -->
			<p class="field-error" role="alert">{ob.lastErrLine}</p>
		{/if}

		{#if ob.installDone}
			<div class="connect-success">
				<CrewFlag size={18} />
				<Check size={15} />
				<span>Installed — connecting…</span>
			</div>
		{/if}

		<!-- Disclosure: prefer terminal -->
		<details class="terminal-disclosure">
			<summary>Prefer the terminal?</summary>
			<div class="code-block">
				<code>atlas crew install {shownTarget}</code>
				<CopyButton
					value="atlas crew install {shownTarget}"
					label=""
					size="sm"
					aria-label="Copy install command"
				/>
			</div>
			<p class="disclosure-hint">Run on your Mac. It SSHes into the server and sets everything up.</p>
		</details>
	{/if}
</div>

<style>
	.form-block {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 28px;
	}

	.code-block {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--ca-surface-sunken);
		border: 1px solid var(--ca-border-subtle);
		border-radius: 8px;
		padding: 11px 14px;
		font-family: monospace;
		font-size: 13px;
		color: var(--ca-text-primary);
	}

	.code-block code {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.field-wrap {
		border-radius: 8px;
	}

	.field-wrap.shake {
		animation: field-shake 420ms ease;
	}

	@keyframes field-shake {
		0%,
		100% {
			transform: translateX(0);
		}
		20%,
		60% {
			transform: translateX(-6px);
		}
		40%,
		80% {
			transform: translateX(6px);
		}
	}

	.port-row {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.port-input {
		width: 90px;
		padding: 8px 10px;
		border-radius: 8px;
		border: 1.5px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		font-size: 14px;
		outline: none;
		transition: border-color 150ms;
	}

	.port-input:focus {
		border-color: var(--ca-accent, #4f8ef7);
	}

	.port-input:disabled {
		opacity: 0.5;
	}

	.connect-success {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 14px;
		color: var(--ca-semantic-success);
	}

	.key-checking {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 14px;
		color: var(--ca-text-secondary);
		padding: 8px 0;
	}

	.key-missing-text {
		font-size: 15px;
		line-height: 1.55;
		color: var(--ca-text-secondary);
		margin: 0 0 20px;
	}

	.auth-block {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.auth-desc {
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	.terminal-disclosure {
		margin-top: 4px;
	}

	.terminal-disclosure summary {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		cursor: pointer;
		user-select: none;
		list-style: none;
		padding: 4px 0;
	}

	.terminal-disclosure summary::-webkit-details-marker {
		display: none;
	}

	.terminal-disclosure[open] summary {
		color: var(--ca-text-primary);
		margin-bottom: 8px;
	}

	.disclosure-hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 6px 0 0;
		line-height: 1.5;
	}
</style>
