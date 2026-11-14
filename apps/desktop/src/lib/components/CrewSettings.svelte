<script lang="ts">
	import { onMount } from "svelte";
	import { Button, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import PairQr from "#lib/components/PairQr.svelte";
	import TelegramRow from "#lib/crew/settings/TelegramRow.svelte";
	import { secondsLeft, type PairStart } from "#lib/settings/mobile.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import {
		loadCrewSettings,
		saveCrewSettings,
		type CrewServerSettings,
	} from "#lib/crew/settings/crewSettings.ts";
	import { replayCrewOnboarding } from "#lib/crew/crewOnboarding.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import type { AgentIntegration } from "#lib/stores/agentIntegration.ts";

	type Device = { node_id: string; name: string; revoked: boolean };

	let hostIntegrations = $state<Record<string, AgentIntegration[]>>({});
	let pairings = $state<Record<string, { p: PairStart; at: number } | null>>({});
	let hostDevices = $state<Record<string, Device[]>>({});
	let crewSettings = $state<Record<string, CrewServerSettings>>({});
	let crewSettingsSaving = $state<Record<string, boolean>>({});
	let now = $state(Date.now());
	let addServer = $state("");
	let addName = $state("");
	let adding = $state(false);

	const remoteServers = $derived(crew.servers.filter((h) => h.key !== ""));

	async function loadHost(key: string) {
		const [intResult, devResult, crewResult] = await Promise.allSettled([
			crew.getServerIntegrations(key),
			hostCall<{ devices: Device[] }>("mobile_devices", { host: key }),
			loadCrewSettings(key),
		]);
		if (intResult.status === "fulfilled") {
			hostIntegrations = { ...hostIntegrations, [key]: intResult.value };
		}
		if (devResult.status === "fulfilled") {
			hostDevices = { ...hostDevices, [key]: devResult.value.devices };
		}
		if (crewResult.status === "fulfilled") {
			crewSettings = { ...crewSettings, [key]: crewResult.value };
		}
	}

	onMount(() => {
		for (const h of remoteServers) {
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

	async function doAddServer() {
		const target = addServer.trim();
		if (!target) return;
		adding = true;
		try {
			const name = addName.trim() || target.split("@").pop() || target;
			await crew.addServer(target, name);
			const key = `ssh:${target}`;
			await loadHost(key).catch(() => {});
			addServer = "";
			addName = "";
			toast.success(`Added ${name}`);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			adding = false;
		}
	}

	async function doRemoveServer(key: string, name: string) {
		if (!confirm(`Remove "${name}"? Crewmates on this server will stop being managed from here.`)) return;
		try {
			await crew.removeServer(key);
			toast.success(`Removed ${name}`);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function saveServerSettings(key: string, short_replies: boolean, short_replies_model: string) {
		crewSettingsSaving = { ...crewSettingsSaving, [key]: true };
		try {
			await saveCrewSettings(key, { short_replies, short_replies_model });
			crewSettings = { ...crewSettings, [key]: { short_replies, short_replies_model } };
		} catch (e) {
			toast.error(errText(e));
		} finally {
			crewSettingsSaving = { ...crewSettingsSaving, [key]: false };
		}
	}
</script>

<PageHead
	title="Atlas Crew"
	description="Crew servers run crewmates 24/7 on your own server. Add a crew server after running `atlas crew install user@host`."
/>

{#each remoteServers as h (h.key)}
	{@const online = crew.serverOk(h.key)}
	{@const integrations = hostIntegrations[h.key] ?? []}
	{@const devices = (hostDevices[h.key] ?? []).filter((d) => !d.revoked)}
	{@const pairing = pairings[h.key] ?? null}
	{@const left = pairing ? secondsLeft(pairing.p, pairing.at, now) : 0}

	<Group title={h.name}>
		<Row id="crew-status-{h.key}" label="Status">
			<span class="status" class:online>
				{online ? "Online" : "Offline"}
			</span>
		</Row>

		{#if integrations.length > 0}
			<Row id="crew-integrations-{h.key}" label="Agents" stack>
				<div class="integrations">
					{#each integrations as a (a.id)}
						<span class="integration" class:ok={a.state === "ok"}>
							{a.label}{a.state === "ok" ? ": logged in" : ": not logged in"}
						</span>
					{/each}
				</div>
			</Row>
		{/if}

		<Row id="crew-phones-{h.key}" label="Paired phones" stack>
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

		{@const cs = crewSettings[h.key]}
		{#if cs}
			<Row
				id="short-replies"
				label="Short replies"
				description="Summarize long crewmate replies with a cheap model."
			>
				<div class="short-replies-form">
					<label class="sr-label">
						<input
							type="checkbox"
							checked={cs.short_replies}
							onchange={(e) =>
								void saveServerSettings(
									h.key,
									(e.currentTarget as HTMLInputElement).checked,
									cs.short_replies_model,
								)}
						/>
						Enabled
					</label>
					{#if cs.short_replies}
						<input
							class="input"
							type="text"
							placeholder="Model id (e.g. cc/claude-haiku-5-5)"
							bind:value={cs.short_replies_model}
							onblur={() => void saveServerSettings(h.key, cs.short_replies, cs.short_replies_model)}
							aria-label="Summary model"
						/>
					{/if}
				</div>
			</Row>
		{/if}

		{#if online}
			<TelegramRow hostKey={h.key} />
		{/if}

		<Row
			id="crew-remove-{h.key}"
			label="Remove server"
			description="Stops managing crewmates on this server."
		>
			<Button size="sm" variant="ghost" onclick={() => doRemoveServer(h.key, h.name)}>Remove</Button>
		</Row>
	</Group>
{/each}

<Group title="Add server">
	<Row
		id="crew-add"
		label="SSH target"
		description="Run `atlas crew install user@host` first, then add the server here."
		stack
	>
		<div class="add-form">
			<input
				class="input"
				type="text"
				placeholder="atlas-crew@server"
				bind:value={addServer}
				onkeydown={(e) => e.key === "Enter" && void doAddServer()}
				aria-label="SSH target"
			/>
			<input
				class="input"
				type="text"
				placeholder="Display name (optional)"
				bind:value={addName}
				onkeydown={(e) => e.key === "Enter" && void doAddServer()}
				aria-label="Display name"
			/>
			<Button size="sm" onclick={doAddServer} disabled={adding || !addServer.trim()}>
				{adding ? "Adding…" : "Add"}
			</Button>
		</div>
	</Row>
</Group>

<Group title="GitHub">
	<Row
		id="crew-github"
		label="GitHub"
		description="Connect GitHub so the crew can clone private repositories. Configure in the crew workspace footer."
	>
		<Button
			size="sm"
			variant="ghost"
			onclick={() => {
				updates.open = false;
			}}>Open crew workspace</Button
		>
	</Row>
</Group>

<Group title="Onboarding">
	<Row
		id="crew-onboarding"
		label="Replay onboarding"
		description="Shows the Atlas Crew introduction again from the first step."
	>
		<Button
			size="sm"
			variant="ghost"
			onclick={() => {
				updates.open = false;
				replayCrewOnboarding();
			}}>Replay</Button
		>
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
	.short-replies-form {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.sr-label {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
</style>
