<script lang="ts">
	import { onMount } from "svelte";
	import { Button, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import PairQr from "#lib/components/PairQr.svelte";
	import { secondsLeft, type PairStart } from "#lib/settings/mobile.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import type { AgentIntegration } from "#lib/stores/agentIntegration.ts";

	type Device = { node_id: string; name: string; revoked: boolean };

	let hostIntegrations = $state<Record<string, AgentIntegration[]>>({});
	let pairings = $state<Record<string, { p: PairStart; at: number } | null>>({});
	let hostDevices = $state<Record<string, Device[]>>({});
	let now = $state(Date.now());
	let addHost = $state("");
	let addName = $state("");
	let adding = $state(false);

	const remoteHosts = $derived(bots.hosts.filter((h) => h.key !== ""));

	async function loadHost(key: string) {
		const [intResult, devResult] = await Promise.allSettled([
			bots.getHostIntegrations(key),
			hostCall<{ devices: Device[] }>("mobile_devices", { host: key }),
		]);
		if (intResult.status === "fulfilled") {
			hostIntegrations = { ...hostIntegrations, [key]: intResult.value };
		}
		if (devResult.status === "fulfilled") {
			hostDevices = { ...hostDevices, [key]: devResult.value.devices };
		}
	}

	onMount(() => {
		for (const h of remoteHosts) {
			void loadHost(h.key).catch(() => {});
		}
		const t = setInterval(() => {
			now = Date.now();
			// Keep QR codes ticking; poll for new pairings while a code is live.
			for (const [key, p] of Object.entries(pairings)) {
				if (p && secondsLeft(p.p, p.at, now) > 0) {
					void loadHost(key).catch(() => {});
				}
			}
		}, 1000);
		return () => clearInterval(t);
	});

	async function startPair(key: string) {
		try {
			const p = await hostCall<PairStart>("mobile_pair_start", { host: key });
			pairings = { ...pairings, [key]: { p, at: Date.now() } };
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function doAddHost() {
		const target = addHost.trim();
		if (!target) return;
		adding = true;
		try {
			const name = addName.trim() || target.split("@").pop() || target;
			await bots.addHost(target, name);
			const key = `ssh:${target}`;
			await loadHost(key).catch(() => {});
			addHost = "";
			addName = "";
			toast.success(`Added ${name}`);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			adding = false;
		}
	}

	async function doRemoveHost(key: string, name: string) {
		if (!confirm(`Remove "${name}"? Bots on this server will stop being managed from here.`)) return;
		try {
			await bots.removeHost(key);
			toast.success(`Removed ${name}`);
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

<PageHead
	title="Bots"
	description="Bot computers run agents 24/7 on your own server. Add a server after running `atlas bot install user@host`."
/>

{#each remoteHosts as h (h.key)}
	{@const online = bots.hostOk(h.key)}
	{@const integrations = hostIntegrations[h.key] ?? []}
	{@const devices = (hostDevices[h.key] ?? []).filter((d) => !d.revoked)}
	{@const pairing = pairings[h.key] ?? null}
	{@const left = pairing ? secondsLeft(pairing.p, pairing.at, now) : 0}

	<Group title={h.name}>
		<Row id="bots-status-{h.key}" label="Status">
			<span class="status" class:online>
				{online ? "● Online" : "○ Offline"}
			</span>
		</Row>

		{#if integrations.length > 0}
			<Row id="bots-integrations-{h.key}" label="Agents" stack>
				<div class="integrations">
					{#each integrations as a (a.id)}
						<span class="integration" class:ok={a.state === "ok"}>
							{a.label}{a.state === "ok" ? ": ✓" : ": not logged in"}
						</span>
					{/each}
				</div>
			</Row>
		{/if}

		<Row id="bots-phones-{h.key}" label="Paired phones" stack>
			{#if devices.length > 0}
				<div class="devices">
					{#each devices as d (d.node_id)}
						<span class="device">{d.name}</span>
					{/each}
				</div>
			{:else}
				<span class="empty">None yet</span>
			{/if}
			{#snippet below()}
				<div class="pair-row">
					<Button size="sm" onclick={() => startPair(h.key)}>
						{pairing && left > 0 ? "New code" : "Pair another phone"}
					</Button>
				</div>
				{#if pairing}
					<PairQr {pairing} {now} />
				{/if}
			{/snippet}
		</Row>

		<Row id="bots-remove-{h.key}" label="Remove server" description="Stops managing bots on this server.">
			<Button size="sm" variant="ghost" onclick={() => doRemoveHost(h.key, h.name)}>Remove</Button>
		</Row>
	</Group>
{/each}

<Group title="Add server">
	<Row
		id="bots-add"
		label="SSH target"
		description="Run `atlas bot install user@host` first, then add the server here."
		stack
	>
		<div class="add-form">
			<input
				class="input"
				type="text"
				placeholder="atlas-bot@server"
				bind:value={addHost}
				onkeydown={(e) => e.key === "Enter" && void doAddHost()}
				aria-label="SSH target"
			/>
			<input
				class="input"
				type="text"
				placeholder="Display name (optional)"
				bind:value={addName}
				onkeydown={(e) => e.key === "Enter" && void doAddHost()}
				aria-label="Display name"
			/>
			<Button size="sm" onclick={doAddHost} disabled={adding || !addHost.trim()}>
				{adding ? "Adding…" : "Add"}
			</Button>
		</div>
	</Row>
</Group>

<style>
	.status {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.status.online {
		color: var(--ca-semantic-success);
	}
	.integrations {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.integration {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.integration.ok {
		color: var(--ca-text-primary);
	}
	.devices {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.device {
		font-size: 12px;
		color: var(--ca-text-primary);
	}
	.empty {
		font-size: 12px;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
	}
	.pair-row {
		margin-top: 8px;
	}
	.add-form {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		align-items: center;
	}
	.input {
		flex: 1;
		min-width: 160px;
		height: 28px;
		padding: 0 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base, transparent);
		color: var(--ca-text-primary);
		font-size: 12px;
		outline: none;
	}
	.input:focus {
		border-color: var(--ca-brand-main);
	}
	.input::placeholder {
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
	}
</style>
