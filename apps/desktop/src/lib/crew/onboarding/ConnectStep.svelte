<!--
  Connect step form: where the server is (address, port), how to sign in (SSH key, which one, or the account password
  once), what the server needs, and what to do when it fails. The platform-neutral wording is deliberate: this runs on
  macOS, Linux and Windows.
-->
<script lang="ts">
	import { Check, CircleAlert } from "@lucide/svelte";
	import { Checkbox, CopyButton, SegmentedControl, TextField } from "@codeatlasdev/ui";
	import CrewFlag from "#lib/crew/CrewFlag.svelte";
	import { browsersCost } from "#lib/crew/browsers.ts";
	import { IS_WIN } from "#lib/keys.ts";
	import { api } from "#lib/api.ts";
	import { fixDocs } from "./docsLinks.ts";
	import { describeConnectError, type ConnectAction } from "./connectError.ts";
	import type { Onboarding } from "./onboarding.svelte.ts";
	import SshHostPicker from "./SshHostPicker.svelte";
	import { m } from "#lib/i18n/m.ts";

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
	let failureEl = $state<HTMLElement>();
	// A failure appears at the bottom of a scrolling form: bring all of it into view, above the footer.
	$effect(() => {
		if (!failureEl) return;
		const frame = requestAnimationFrame(() =>
			failureEl?.scrollIntoView({ block: "end", behavior: "smooth" }),
		);
		return () => cancelAnimationFrame(frame);
	});
	/** The button of each fix a failure offers (`sudo` has none: its field is below). */
	function actionLabel(a: ConnectAction): string {
		switch (a) {
			case "pick_key":
				return m.crew_onboarding_connect_step_action_pick_key();
			case "use_password":
				return m.crew_onboarding_connect_step_action_use_password();
			case "use_key":
				return m.crew_onboarding_connect_step_action_use_key();
			case "retry":
				return m.crew_onboarding_connect_step_action_retry();
			case "recheck":
				return m.crew_onboarding_check_again();
			case "sudo":
				return "";
		}
	}
</script>

<div class="form-block" style:--stagger={3}>
	{#if !ob.keyChecked}
		<div class="key-checking">
			<span class="spin-dot" aria-hidden="true"></span>
			<span>{m.crew_onboarding_connect_step_looking_for_your_ssh_key()}</span>
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
				<!-- i18n-ignore: an example user@host, not prose -->
				<TextField
					name="ssh-target"
					label={m.crew_onboarding_connect_step_ssh_address()}
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
					label={m.crew_onboarding_connect_step_port()}
					type="number"
					value={String(ob.sshPort)}
					oninput={(v) => (ob.sshPort = Number(v) || 22)}
					disabled={locked}
				/>
			</div>
		</div>

		<!-- How: one segmented control, and below it the field it reveals, in an area of the same height for both. -->
		<div class="group">
			<span class="group-label">{m.crew_onboarding_connect_step_sign_in_with()}</span>
			<SegmentedControl
				aria-label={m.crew_onboarding_connect_step_sign_in_with()}
				value={ob.method}
				items={[
					{ value: "key", label: m.crew_onboarding_connect_step_method_key() },
					{ value: "password", label: m.crew_onboarding_connect_step_method_password() },
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
								{m.crew_onboarding_connect_step_choose_another()}
							</button>
						</div>
						<p class="field-hint">
							{ob.identity
								? m.crew_onboarding_connect_step_key_kept()
								: m.crew_onboarding_connect_step_key_in_use()}
							{#if ob.identity}
								<button class="text-link" onclick={() => ob.useDefaultKey()} disabled={locked}>
									{m.crew_onboarding_connect_step_use_the_default()}
								</button>
							{/if}
						</p>
					{:else}
						<div class="key-row" data-testid="no-key">
							<span class="key-path">
								{ob.identity
									? m.crew_onboarding_connect_step_key_file_missing()
									: m.crew_onboarding_connect_step_no_key()}
							</span>
							<button class="text-link" onclick={() => ob.createSshKey()} disabled={ob.creatingKey || locked}>
								{ob.creatingKey
									? m.crew_onboarding_connect_step_creating()
									: m.crew_onboarding_connect_step_create_one()}
							</button>
							<button class="text-link" onclick={() => ob.pickKeyFile()} disabled={locked}>
								{m.crew_onboarding_connect_step_choose_a_file()}
							</button>
						</div>
						<p class="field-hint">{m.crew_onboarding_connect_step_password_creates_key()}</p>
					{/if}
				{:else}
					<TextField
						name="ssh-password"
						label={m.crew_onboarding_connect_step_password_of({ user: shownTarget.split("@")[0] })}
						hideLabel
						placeholder={m.crew_onboarding_connect_step_password_of({ user: shownTarget.split("@")[0] })}
						type="password"
						bind:value={ob.sshPassword}
						disabled={locked}
						autocomplete="off"
						onkeydown={(e) => e.key === "Enter" && ob.canSubmit && ob.launchServer()}
					/>
					<p class="field-hint">{m.crew_onboarding_connect_step_password_used_once()}</p>
				{/if}
			</div>
		</div>

		<!-- Optional: one compact row (out of the way while a failure needs the room). -->
		<div class="optional" class:gone={failure !== null && failure !== undefined}>
			<Checkbox
				bind:checked={ob.browsers}
				label={m.crew_onboarding_connect_step_browsers_option()}
				disabled={locked}
			/>
			<p class="field-hint">{browsersCost()}</p>
		</div>

		{#if failure}
			<div
				class="failure"
				bind:this={failureEl}
				role="alert"
				data-testid="connect-failure"
				data-kind={failure.kind}
			>
				<CircleAlert size={16} class="failure-icon" />
				<div class="failure-main">
					<p class="failure-title">{failure.title}</p>
					<p class="failure-body">{failure.body}</p>
					{#if failure.command}
						<div class="code-block">
							<code>{failure.command}</code>
							<CopyButton
								value={failure.command}
								label=""
								size="sm"
								aria-label={m.crew_onboarding_connect_step_copy_command()}
							/>
						</div>
					{/if}
					{#if ob.needsSudo}
						<TextField
							name="sudo-password"
							label={m.crew_onboarding_connect_step_sudo_password()}
							hideLabel
							placeholder={m.crew_onboarding_connect_step_sudo_password()}
							type="password"
							bind:value={ob.sudoPassword}
							disabled={ob.installRunning}
							autocomplete="off"
							onkeydown={(e) => e.key === "Enter" && ob.sudoPassword && ob.launchServer(ob.sudoPassword)}
						/>
					{/if}
					<!-- The fixes as buttons, the docs as a link, on one row. -->
					<div class="failure-actions">
						{#each failure.actions.filter((a) => a !== "sudo") as a (a)}
							<button class="act" onclick={() => ob.runAction(a)} disabled={ob.installRunning}>
								{actionLabel(a)}
							</button>
						{/each}
						<button class="text-link small fix" onclick={() => api.openUrl(fixDocs())}>
							{m.crew_onboarding_connect_step_how_to_fix()}
						</button>
					</div>
					{#if failure.detail}
						<details class="failure-details">
							<summary>{m.crew_onboarding_connect_step_details()}</summary>
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
				<span>{m.crew_onboarding_connect_step_installed_connecting()}</span>
			</div>
		{/if}

		<!-- A small link; the command opens over the layout and pushes nothing. -->
		<div class="terminal">
			<button class="text-link small" onclick={() => (showCmd = !showCmd)} aria-expanded={showCmd}>
				{m.crew_onboarding_connect_step_prefer_the_terminal()}
			</button>
			{#if showCmd}
				<div class="popover">
					<div class="code-block">
						<code>{terminalCmd}</code>
						<CopyButton
							value={terminalCmd}
							label=""
							size="sm"
							aria-label={m.crew_onboarding_connect_step_copy_install_command()}
						/>
					</div>
					<p class="field-hint">{m.crew_onboarding_connect_step_terminal_hint()}</p>
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

	.optional.gone {
		display: none;
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
		align-items: center;
		gap: 8px 10px;
		margin-top: 2px;
	}

	.act {
		padding: 6px 12px;
		border-radius: 8px;
		border: 1px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		font-size: 13px;
		cursor: pointer;
	}

	.act:hover:not(:disabled) {
		border-color: var(--ob-accent, var(--ca-accent));
	}

	.act:disabled {
		opacity: 0.5;
		cursor: default;
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
		font-size: 12.5px;
		padding: 0 4px;
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
