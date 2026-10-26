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

<Group title="Agent hooks">
	{#each agents.integrations as a, i (a.id)}
		<Row
			id={i === 0 ? "agent-integration" : undefined}
			label={a.label}
			description={a.state === "ok"
				? "Reports when it works, waits for you or is done, and can use Atlas tools."
				: a.state === "missing"
					? "Not connected: Atlas and Atlas Mobile cannot tell what it is doing."
					: "Atlas could not tell if its hooks are on."}
		>
			{#snippet lead()}<AgentLogo agent={a.id} size={18} />{/snippet}
			{#if a.state === "ok"}
				<StatusChip status="success" label="On" />
			{:else}
				<Button size="sm" disabled={busy === a.id} onclick={() => turnOn(a.id)}>Turn on</Button>
			{/if}
		</Row>
	{:else}
		<Row
			id="agent-integration"
			label="No agents found"
			description="Install omp, Claude Code or Codex and they show up here, ready to connect."
		/>
	{/each}
</Group>
