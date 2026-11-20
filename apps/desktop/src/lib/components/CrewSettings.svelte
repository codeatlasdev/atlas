<script lang="ts">
	import { onMount } from "svelte";
	import { Button, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { hostCall } from "#lib/host.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import PairQr from "#lib/components/PairQr.svelte";
	import BrowsersRow from "#lib/crew/settings/BrowsersRow.svelte";
	import TelegramRow from "#lib/crew/settings/TelegramRow.svelte";
	import GithubAppCard from "#lib/crew/settings/GithubAppCard.svelte";
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
	import { m } from "#lib/i18n/m.ts";

	type Device = { node_id: string; name: string; revoked: boolean };

	let hostIntegrations = $state<Record<string, AgentIntegration[]>>({});
	let pairings = $state<Record<string, { p: PairStart; at: number } | null>>({});
	let hostDevices = $state<Record<string, Device[]>>({});
	let crewSettings = $state<Record<string, CrewServerSettings>>({});
	let crewSettingsSaving = $state<Record<string, boolean>>({});
	let pausing = $state<Record<string, boolean>>({});
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
			toast.success(m.settings_agents_crew_added({ name }));
		} catch (e) {
			toast.error(errText(e));
		} finally {
			adding = false;
		}
	}

	async function doRemoveServer(key: string, name: string) {
		if (!confirm(m.settings_agents_crew_remove_confirm({ name }))) return;
		try {
			await crew.removeServer(key);
			toast.success(m.settings_agents_crew_removed({ name }));
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function togglePause(key: string, pause: boolean) {
		pausing = { ...pausing, [key]: true };
		try {
			await (pause ? crew.pauseServer(key) : crew.resumeServer(key));
		} catch (e) {
			toast.error(errText(e));
		} finally {
			pausing = { ...pausing, [key]: false };
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

<PageHead title={m.settings_agents_crew_title()} description={m.settings_agents_crew_description()} />

{#each remoteServers as h (h.key)}
	{@const online = crew.serverOk(h.key)}
	{@const integrations = hostIntegrations[h.key] ?? []}
	{@const devices = (hostDevices[h.key] ?? []).filter((d) => !d.revoked)}
	{@const pairing = pairings[h.key] ?? null}
	{@const left = pairing ? secondsLeft(pairing.p, pairing.at, now) : 0}

	<Group title={h.name}>
		<Row id="crew-status-{h.key}" label={m.settings_agents_crew_status()}>
			<span class="status" class:online>
				{online ? m.settings_agents_crew_status_online() : m.settings_agents_crew_status_offline()}
			</span>
		</Row>

		<Row
			id="crew-paused-{h.key}"
			label={h.paused ? m.crew_workspace_sidebar_footer_paused() : m.settings_agents_crew_pause_label()}
			description={h.paused
				? m.settings_agents_crew_pause_hint_paused()
				: m.settings_agents_crew_pause_hint_running()}
		>
			<Button
				size="sm"
				variant={h.paused ? "filled" : "ghost"}
				disabled={!online || pausing[h.key]}
				onclick={() => void togglePause(h.key, !h.paused)}
			>
				{h.paused ? m.crew_workspace_pause_banner_resume() : m.crew_workspace_sidebar_footer_pause_all()}
			</Button>
		</Row>

		{#if integrations.length > 0}
			<Row id="crew-integrations-{h.key}" label={m.settings_agents_crew_agents()} stack>
				<div class="integrations">
					{#each integrations as a (a.id)}
						<span class="integration" class:ok={a.state === "ok"}>
							{a.state === "ok"
								? m.settings_agents_crew_agent_signed_in({ label: a.label })
								: m.settings_agents_crew_agent_signed_out({ label: a.label })}
						</span>
					{/each}
				</div>
			</Row>
		{/if}

		<Row id="crew-phones-{h.key}" label={m.settings_agents_paired_phones()} stack>
			{#if devices.length > 0}
				<div class="devices">
					{#each devices as d (d.node_id)}
						<span class="device">{d.name}</span>
					{/each}
				</div>
			{:else}
				<span class="empty">{m.settings_agents_crew_phones_empty()}</span>
			{/if}
			{#snippet below()}
				<div class="pair-row">
					<Button size="sm" onclick={() => startPair(h.key)}>
						{pairing && left > 0 ? m.mobile_pair_card_new_code() : m.mobile_pair_card_pair_another()}
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
				label={m.crew_settings_behaviour_card_short_replies()}
				description={m.crew_settings_behaviour_card_summarize_long_replies_with_a()}
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
						{m.settings_agents_crew_enabled()}
					</label>
					{#if cs.short_replies}
						<input
							class="input"
							type="text"
							placeholder={m.settings_agents_crew_model_placeholder()}
							bind:value={cs.short_replies_model}
							onblur={() => void saveServerSettings(h.key, cs.short_replies, cs.short_replies_model)}
							aria-label={m.settings_agents_crew_summary_model()}
						/>
					{/if}
				</div>
			</Row>
		{/if}

		{#if online}
			<BrowsersRow hostKey={h.key} />
			<TelegramRow hostKey={h.key} />
		{/if}

		<Row
			id="crew-remove-{h.key}"
			label={m.settings_agents_crew_remove_server()}
			description={m.settings_agents_crew_remove_hint()}
		>
			<Button size="sm" variant="ghost" onclick={() => doRemoveServer(h.key, h.name)}
				>{m.common_action_remove()}</Button
			>
		</Row>
	</Group>

	{#if online}
		<GithubAppCard hostKey={h.key} serverName={h.name} />
	{/if}
{/each}

<Group title={m.settings_agents_crew_add_server()}>
	<Row
		id="crew-add"
		label={m.settings_agents_crew_target()}
		description={m.settings_agents_crew_target_description()}
		stack
	>
		<div class="add-form">
			<!-- i18n-ignore: an example user@host, not prose -->
			<input
				class="input"
				type="text"
				placeholder="atlas-crew@server"
				bind:value={addServer}
				onkeydown={(e) => e.key === "Enter" && void doAddServer()}
				aria-label={m.settings_agents_crew_target()}
			/>
			<input
				class="input"
				type="text"
				placeholder={m.settings_agents_crew_display_name_optional()}
				bind:value={addName}
				onkeydown={(e) => e.key === "Enter" && void doAddServer()}
				aria-label={m.settings_agents_crew_display_name()}
			/>
			<Button size="sm" onclick={doAddServer} disabled={adding || !addServer.trim()}>
				{adding ? m.settings_agents_crew_adding() : m.common_action_add()}
			</Button>
		</div>
	</Row>
</Group>

<!-- i18n-ignore: a product name -->
<Group title="GitHub">
	<Row
		id="crew-github"
		label={m.crew_github_app_simple_title()}
		description={m.crew_github_app_simple_description()}
	>
		<Button
			size="sm"
			variant="ghost"
			onclick={() => {
				updates.open = false;
			}}>{m.settings_agents_crew_open_workspace()}</Button
		>
	</Row>
</Group>

<Group title={m.settings_agents_crew_onboarding()}>
	<Row
		id="crew-onboarding"
		label={m.settings_agents_crew_replay_onboarding()}
		description={m.settings_agents_crew_replay_description()}
	>
		<Button
			size="sm"
			variant="ghost"
			onclick={() => {
				updates.open = false;
				replayCrewOnboarding();
			}}>{m.settings_agents_crew_replay()}</Button
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
