<script lang="ts">
	import { onMount } from "svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { Button, Select, Spinner, StatusChip, TextField, toast } from "@codeatlasdev/ui";
	import { api, errText } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import Row from "#lib/settings/Row.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";
	import { sshStatusView } from "#lib/settings/sshStatus.ts";
	import { m } from "#lib/i18n/m.ts";

	let aliases = $state<string[]>([]);
	let hosts = $state<string[]>([]);
	let status = $state<Record<string, string>>({});
	let pick = $state("");
	let tests = $state<Record<string, string>>({});
	let addHost = $state("");
	let addPath = $state("");
	let adding = $state(false);

	let loadError = $state<string | null>(null);

	async function load() {
		try {
			aliases = await invoke<string[]>("ssh_config_hosts");
			const s = await api.settings();
			hosts = Array.isArray(s.sshHosts) ? (s.sshHosts as { alias: string }[]).map((h) => h.alias) : [];
			status = await invoke<Record<string, string>>("ssh_status");
			addHost ||= hosts[0] ?? "";
			loadError = null;
		} catch (e) {
			loadError = errText(e);
		}
	}

	onMount(() => {
		void load();
		const off = listen<{ alias: string; status: string }>(
			"remote://status",
			(e) => (status[e.payload.alias] = e.payload.status),
		);
		return () => void off.then((u) => u());
	});

	async function add() {
		if (!pick) return;
		await invoke("ssh_add", { alias: pick }).catch((e) => toast.error(errText(e)));
		pick = "";
		await load();
	}

	async function test(alias: string) {
		tests[alias] = "…";
		try {
			const r = await invoke<{ home: string; arch: string; os: string; atlas: string | null }>("ssh_test", {
				alias,
			});
			tests[alias] = r.atlas
				? m.settings_agents_ssh_test_result({ os: r.os, arch: r.arch, atlas: r.atlas })
				: m.settings_agents_ssh_test_atlas_missing({ os: r.os, arch: r.arch });
		} catch (e) {
			tests[alias] = errText(e);
		}
	}

	async function remove(alias: string) {
		await invoke("ssh_remove", { alias }).catch((e) => toast.error(errText(e)));
		await load();
	}

	async function addProject() {
		if (!addHost || !addPath.trim()) return;
		adding = true;
		try {
			await invoke("projects_add_remote", { alias: addHost, root: addPath.trim() });
			await store.refresh();
			addPath = "";
			toast.success(m.settings_agents_ssh_project_added());
		} catch (e) {
			toast.error(errText(e));
		} finally {
			adding = false;
		}
	}
</script>

{#if loadError}
	<LoadError what="your SSH hosts" error={loadError} />
{/if}

<Group title={m.settings_agents_ssh_your_hosts()}>
	{#each hosts as h (h)}
		{@const view = sshStatusView(status[h])}
		<Row label={h} description={tests[h] ?? undefined}>
			<StatusChip status={view.tone} label={view.label} />
			{#if status[h] === "connecting"}<Spinner size={11} />{/if}
			<Button size="sm" variant="outlined" onclick={() => test(h)}>{m.crew_settings_engine_section_test_connection()}</Button>
			<Button size="sm" variant="ghost" onclick={() => remove(h)}>{m.common_action_remove()}</Button>
			{#snippet below()}
				{#if view.detail}<div class="err ca-selectable">
						{view.detail}
					</div>{/if}
			{/snippet}
		</Row>
	{:else}
		<Row
			id="ssh-hosts"
			label={m.settings_agents_ssh_no_hosts()}
			description={m.settings_agents_ssh_no_hosts_description()}
		/>
	{/each}
	<Row
		id={hosts.length ? "ssh-hosts" : undefined}
		label={m.settings_agents_ssh_add_host_label()}
		description={m.settings_agents_ssh_add_host_description()}
	>
		<div class="select">
			<Select
				size="sm"
				placeholder={m.settings_agents_ssh_pick_placeholder()}
				value={pick}
				options={aliases.filter((a) => !hosts.includes(a)).map((a) => ({ value: a, label: a }))}
				onchange={(v) => (pick = v)}
			/>
		</div>
		<Button size="sm" disabled={!pick} onclick={add}>{m.settings_agents_ssh_add_host()}</Button>
	</Row>
</Group>

{#if hosts.length}
	<Group
		title={m.settings_agents_ssh_remote_project()}
		description={m.settings_agents_ssh_remote_project_description()}
	>
		<Row id="ssh-project" label={m.settings_agents_ssh_host()} description={m.settings_agents_ssh_host_description()}>
			<div class="select">
				<Select
					size="sm"
					value={addHost}
					options={hosts.map((h) => ({ value: h, label: h }))}
					onchange={(v) => (addHost = v)}
				/>
			</div>
		</Row>
		<Row label={m.settings_agents_ssh_project_folder()} description={m.settings_agents_ssh_project_folder_description()} stack>
			<!-- i18n-ignore: an example path, not prose -->
			<TextInput
				mono
				width="100%"
				label={m.settings_agents_ssh_project_folder()}
				placeholder="/home/you/project"
				value={addPath}
				onchange={(v) => (addPath = v)}
			/>
		</Row>
		<Row label={m.settings_agents_ssh_add_project_label()} description={m.settings_agents_ssh_add_project_description()}>
			<Button size="sm" disabled={adding || !addPath.trim()} onclick={addProject}>{m.settings_agents_ssh_add_project()}</Button>
		</Row>
	</Group>
{/if}

<style>
	.select {
		width: 180px;
	}
	.err {
		font-size: 11.5px;
		color: var(--ca-semantic-error, #d33);
		white-space: pre-wrap;
	}
</style>
