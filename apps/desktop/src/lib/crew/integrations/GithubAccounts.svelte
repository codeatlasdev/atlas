<script lang="ts">
	/**
	 * GithubAccounts — per crew server GitHub connection of the GitHub integration page: connect (device flow),
	 * connected-as, disconnect.
	 */
	import { Button } from "@codeatlasdev/ui";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import { crewServerLabel } from "#lib/crew/sshTarget.ts";
	import GithubDeviceFlow from "#lib/crew/GithubDeviceFlow.svelte";
	import IntegrationCard from "#lib/crew/integrations/IntegrationCard.svelte";
</script>

<IntegrationCard title="GitHub accounts on your crew servers">
	{#each crew.servers.filter((h) => h.key.startsWith("ssh:")) as server (server.key)}
		{@const status = githubCrew.statuses.get(server.key)}
		{@const flow = githubCrew.flows.get(server.key)}
		{@const polling = githubCrew.isPolling(server.key)}
		<div class="server-gh-row">
			<span class="server-gh-name">{crewServerLabel(server.name)}</span>
			{#if flow}
				<!-- Login flow in progress -->
				<GithubDeviceFlow {flow} {polling} oncancel={() => githubCrew.stopPolling(server.key)} />
			{:else if status?.login}
				<!-- Connected -->
				<span class="server-gh-login">Connected as @{status.login}</span>
				<Button variant="ghost" size="sm" onclick={() => void githubCrew.logout(server.key)}
					>Disconnect</Button
				>
			{:else}
				<!-- Not connected -->
				<span class="server-gh-status-off">Not connected</span>
				<Button variant="filled" size="sm" onclick={() => void githubCrew.startLogin(server.key)}
					>Connect</Button
				>
			{/if}
		</div>
	{:else}
		<p class="no-servers">
			No crew servers added yet. Add a crew server in the Atlas Crew tab to connect GitHub.
		</p>
	{/each}
</IntegrationCard>

<style>
	.server-gh-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 14px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	.server-gh-row:last-child {
		border-bottom: none;
	}
	.server-gh-name {
		flex: 1;
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.server-gh-login {
		font-size: 13px;
		color: var(--ca-text-secondary);
	}
	.server-gh-status-off {
		font-size: 13px;
		color: var(--ca-text-disabled);
	}

	.no-servers {
		font-size: 13px;
		color: var(--ca-text-secondary);
		padding: 12px 14px;
		margin: 0;
	}
</style>
