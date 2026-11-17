<!--
  Connect step form: where the server is (address, port), how to sign in (SSH key, which one, or the account password
  once), what the server needs, and what to do when it fails. The platform-neutral wording is deliberate: this runs on
  macOS, Linux and Windows.
-->
<script lang="ts">
	import { Check, CircleAlert } from "@lucide/svelte";
	import { Checkbox, CopyButton, SegmentedControl, TextField } from "@codeatlasdev/ui";
	import CrewFlag from "#lib/crew/CrewFlag.svelte";
	import { BROWSERS_COST, BROWSERS_WHY } from "#lib/crew/browsers.ts";
	import { IS_WIN } from "#lib/keys.ts";
	import { api } from "#lib/api.ts";
	import { fixDocs } from "./docsLinks.ts";
	import { describeConnectError } from "./connectError.ts";
	import type { Onboarding } from "./onboarding.svelte.ts";
	import SshHostPicker from "./SshHostPicker.svelte";

	let { ob }: { ob: Onboarding } = $props();

	const shownTarget = $derived(ob.sshTarget.trim() || "user@host");
	const locked = $derived(ob.installRunning || ob.installDone);
	const terminalCmd = $derived(
		`atlas crew install ${shownTarget}${ob.identity ? ` --identity "${ob.identity}"` : ""}${ob.browsers ? " --browsers" : ""}`,
	);
	/** Without ssh nothing else on this form can work: say so first, with the way to get it. */
	const noSsh = $derived(
		ob.keyChecked && !ob.sshFound
			? describeConnectError(
					{ kind: "ssh_missing", detail: "ssh: not found" },
					{ keyPath: null, keyAdded: false, target: shownTarget, windows: IS_WIN },
				)
			: null,
	);
	const failure = $derived(noSsh ?? ob.failure);
	let showCmd = $state(false);
	const labels = {
		pick_key: "Choose another key",
		use_password: "Sign in with the password",
		use_key: "Use my SSH key",
		retry: "Try again",
		recheck: "Check again",
		sudo: "",
	} as const;
</script>

<div class="form-block" style:--stagger={3}>
	{#if !ob.keyChecked}
		<div class="key-checking">
			<span class="spin-dot" aria-hidden="true"></span>
			<span>Looking for your SSH key…</span>
		</div>
	{:else}
		{#if ob.sshHosts.length > 0}
			<SshHostPicker
				hosts={ob.sshHosts}
				address={ob.sshTarget}
				port={ob.sshPort}
				disabled={locked}
				onpick={(h) => ob.pickSshHost(h)}
			/>
		{/if}

		<!-- Where: the address and a narrow port on one row. -->
		<div class="row">
			<div class="grow" class:shake={ob.fieldShake}>
				<TextField
					name="ssh-target"
					label="SSH address"
					placeholder="ubuntu@my-server.com"
					bind:value={ob.sshTarget}
					disabled={locked}
					autocomplete="off"
					onkeydown={(e) => e.key === "Enter" && ob.canSubmit && ob.launchServer()}
				/>
			</div>
			<div class="port">
				<TextField
					name="ssh-port"
					label="Port"
					type="number"
					value={String(ob.sshPort)}
					oninput={(v) => (ob.sshPort = Number(v) || 22)}
					disabled={locked}
				/>
			</div>
		</div>

		<!-- How: one segmented control, and below it the field it reveals, in an area of the same height for both. -->
		<div class="group">
			<span class="group-label">Sign in with</span>
			<SegmentedControl
				aria-label="Sign in with"
				value={ob.method}
				items={[
					{ value: "key", label: "SSH key" },
					{ value: "password", label: "Password" },
				]}
				disabled={locked}
				onchange={(v) => ob.chooseMethod(v as "key" | "password")}
			/>
			<div class="reveal">
				{#if ob.method === "key"}
					{#if ob.key}
						<div class="key-row" data-testid="key-in-use">
							<code class="key-path" title={ob.key.identity}>{ob.key.identity}</code>
							<button class="text-link" onclick={() => ob.pickKeyFile()} disabled={locked}>
								Choose another…
							</button>
						</div>
						<p class="field-hint">
							{ob.identity ? "Used for this server from now on." : "The key ssh uses for this address."}
							{#if ob.identity}
								<button class="text-link" onclick={() => ob.useDefaultKey()} disabled={locked}>
									Use the default
								</button>
							{/if}
						</p>
					{:else}
						<div class="key-row" data-testid="no-key">
							<span class="key-path">
								{ob.identity ? "That key file is not there." : "No SSH key on this computer."}
							</span>
							<button class="text-link" onclick={() => ob.createSshKey()} disabled={ob.creatingKey || locked}>
								{ob.creatingKey ? "Creating…" : "Create one"}
							</button>
							<button class="text-link" onclick={() => ob.pickKeyFile()} disabled={locked}
								>Choose a file</button
							>
						</div>
						<p class="field-hint">Or sign in with the password: Atlas creates the key for you.</p>
					{/if}
				{:else}
					<TextField
						name="ssh-password"
						label={`Password of ${shownTarget.split("@")[0]}`}
						hideLabel
						placeholder={`Password of ${shownTarget.split("@")[0]}`}
						type="password"
						bind:value={ob.sshPassword}
						disabled={locked}
						autocomplete="off"
						onkeydown={(e) => e.key === "Enter" && ob.canSubmit && ob.launchServer()}
					/>
					<p class="field-hint">Used once to add this computer's key; never stored.</p>
				{/if}
			</div>
		</div>

		<!-- Optional: one compact row. -->
		<div class="optional">
			<Checkbox
				bind:checked={ob.browsers}
				label="Screenshots for UI work (Chromium + WebKit)"
				disabled={locked}
			/>
			<p class="field-hint">{BROWSERS_COST}</p>
		</div>

		{#if failure}
			<div class="failure" role="alert" data-testid="connect-failure" data-kind={failure.kind}>
				<CircleAlert size={16} class="failure-icon" />
				<div class="failure-main">
					<p class="failure-title">{failure.title}</p>
					<p class="failure-body">{failure.body}</p>
					{#if failure.command}
						<div class="code-block">
							<code>{failure.command}</code>
							<CopyButton value={failure.command} label="" size="sm" aria-label="Copy command" />
						</div>
					{/if}
					{#if failure.actions.some((a) => a !== "sudo")}
						<div class="failure-actions">
							{#each failure.actions.filter((a) => a !== "sudo") as a (a)}
								<button class="text-link" onclick={() => ob.runAction(a)} disabled={ob.installRunning}>
									{labels[a]}
								</button>
							{/each}
						</div>
					{/if}
					{#if ob.needsSudo}
						<TextField
							name="sudo-password"
							label="Sudo password"
							hideLabel
							placeholder="Sudo password"
							type="password"
							bind:value={ob.sudoPassword}
							disabled={ob.installRunning}
							autocomplete="off"
							onkeydown={(e) => e.key === "Enter" && ob.sudoPassword && ob.launchServer(ob.sudoPassword)}
						/>
					{/if}
					<button class="text-link small fix" onclick={() => api.openUrl(fixDocs())}>How to fix</button>
					{#if failure.detail}
						<details class="failure-details">
							<summary>Details</summary>
							<code>{failure.detail}</code>
						</details>
					{/if}
				</div>
			</div>
		{/if}

		{#if ob.installDone}
			<div class="connect-success">
				<CrewFlag size={18} />
				<Check size={15} />
				<span>Installed — connecting…</span>
			</div>
		{/if}

		<!-- A small link; the command opens over the layout and pushes nothing. -->
		<div class="terminal">
			<button class="text-link small" onclick={() => (showCmd = !showCmd)} aria-expanded={showCmd}>
				Prefer the terminal?
			</button>
			{#if showCmd}
				<div class="popover">
					<div class="code-block">
						<code>{terminalCmd}</code>
						<CopyButton value={terminalCmd} label="" size="sm" aria-label="Copy install command" />
					</div>
					<p class="field-hint">Run it on this computer; it uses the same key.</p>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* One scale: 16px between groups, 6px label -> field -> hint inside a group. */
	.form-block {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.group {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.group-label {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}

	.row {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}

	.grow {
		flex: 1;
		min-width: 0;
	}

	.grow.shake {
		animation: field-shake 420ms ease;
	}

	.port {
		width: 7.5em;
		flex-shrink: 0;
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

	/* The same height for the key picker and the password field: switching the method moves nothing below. */
	.reveal {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-height: 84px;
		margin-top: 4px;
	}

	.key-row {
		display: flex;
		align-items: center;
		gap: 12px;
		box-sizing: border-box;
		height: 40px;
		padding: 0 12px;
		border: 1px solid var(--ca-border-subtle);
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		font-size: 13px;
		color: var(--ca-text-secondary);
	}

	.key-path {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}

	.optional {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.optional .field-hint {
		padding-left: 26px;
	}

	.code-block {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--ca-surface-sunken);
		border: 1px solid var(--ca-border-subtle);
		border-radius: 8px;
		padding: 8px 12px;
		font-family: monospace;
		font-size: 12.5px;
		color: var(--ca-text-primary);
	}

	.code-block code {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.failure {
		display: flex;
		gap: 10px;
		padding: 12px 14px;
		border-radius: 8px;
		border: 1px solid var(--ca-danger, #ef4444);
		background: var(--ca-surface-sunken);
	}

	.failure :global(.failure-icon) {
		flex-shrink: 0;
		margin-top: 2px;
		color: var(--ca-danger, #ef4444);
	}

	.failure-main {
		display: flex;
		flex-direction: column;
		gap: 6px;
		min-width: 0;
	}

	.failure-title {
		margin: 0;
		font-size: 14px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}

	.failure-body {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}

	.failure-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 2px 14px;
	}

	.failure-details summary {
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}

	.failure-details code {
		display: block;
		margin-top: 4px;
		font-size: 12px;
		word-break: break-word;
		color: var(--ca-text-secondary);
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

	.fix {
		align-self: flex-start;
		font-size: 12.5px;
		padding: 0;
	}

	.terminal {
		position: relative;
		align-self: flex-start;
	}

	.terminal :global(.small) {
		font-size: 12.5px;
		padding: 0;
	}

	.popover {
		position: absolute;
		left: 0;
		bottom: calc(100% + 6px);
		z-index: 3;
		width: min(480px, 80vw);
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px;
		border-radius: 10px;
		border: 1px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
	}
</style>
