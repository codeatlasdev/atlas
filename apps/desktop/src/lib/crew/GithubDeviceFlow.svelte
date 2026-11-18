<script lang="ts">
	/**
	 * GithubDeviceFlow — the "Enter this code at <uri>" block of the GitHub device-flow
	 * login: verification link, the user code with a copy button, the waiting line
	 * and Cancel. Shared by the integrations catalog and the crewmate chat.
	 */
	import { Button, CopyButton } from "@codeatlasdev/ui";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		flow: { verification_uri: string; user_code: string };
		polling: boolean;
		oncancel: () => void;
	}

	let { flow, polling, oncancel }: Props = $props();
</script>

<div class="gh-flow-block">
	<p class="gh-flow-label">
		{m.crew_workspace_github_device_flow_enter_code_at()}
		<a href={flow.verification_uri} target="_blank" rel="noreferrer" class="text-link"
			>{flow.verification_uri}</a
		>
	</p>
	<div class="gh-code-row">
		<code class="gh-code">{flow.user_code}</code>
		<CopyButton value={flow.user_code} label={m.common_action_copy()} size="sm" />
	</div>
	{#if polling}
		<p class="gh-flow-waiting" role="status" aria-live="polite">
			{m.crew_workspace_github_device_flow_waiting_for_authorization()}
		</p>
	{/if}
	<Button variant="ghost" size="sm" onclick={oncancel}>{m.common_action_cancel()}</Button>
</div>

<style>
	.gh-flow-block {
		width: 100%;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 0;
	}
	.gh-flow-label {
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.gh-code-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.gh-code {
		font-size: 1.5rem;
		font-weight: 700;
		letter-spacing: 0.1em;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		border-radius: 6px;
		padding: 6px 12px;
	}
	.gh-flow-waiting {
		font-size: 11.5px;
		color: var(--ca-text-disabled);
		margin: 0;
	}
	.text-link {
		color: var(--ca-accent, #4f8ef7);
	}
</style>
