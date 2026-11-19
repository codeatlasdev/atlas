<script lang="ts">
	/**
	 * Settings ▸ Integrations ▸ Agent hooks: one row per installed agent saying whether its hooks are on, with
	 * the switch to turn them on. Before this page the rows existed only while a hook was missing, so a user who
	 * had finished could not see that it worked.
	 */
	import { Button, StatusChip } from "@codeatlasdev/ui";
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let busy = $state<string | null>(null);

	async function turnOn(id: string) {
		busy = id;
		try {
			await agents.enableIntegration(id);
		} finally {
			busy = null;
		}
	}
</script>

<Group title={m.settings_general_agent_hooks_agent_hooks()}>
	{#each agents.integrations as a, i (a.id)}
		<Row
			id={i === 0 ? "agent-integration" : undefined}
			label={a.label}
			description={a.state === "ok"
				? m.settings_general_agent_hooks_state_ok()
				: a.state === "missing"
					? m.settings_general_agent_hooks_state_missing()
					: m.settings_general_agent_hooks_state_unknown()}
		>
			{#snippet lead()}<AgentLogo agent={a.id} size={18} />{/snippet}
			{#if a.state === "ok"}
				<StatusChip status="success" label={m.settings_general_agent_hooks_on()} />
			{:else}
				<Button size="sm" disabled={busy === a.id} onclick={() => turnOn(a.id)}
					>{m.settings_general_agent_hooks_turn_on()}</Button
				>
			{/if}
		</Row>
	{:else}
		<Row
			id="agent-integration"
			label={m.settings_general_agent_hooks_no_agents_found()}
			description={m.settings_general_agent_hooks_install_omp_claude_code_or()}
		/>
	{/each}
</Group>
