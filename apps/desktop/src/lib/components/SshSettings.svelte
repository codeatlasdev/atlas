<script lang="ts">
	import { onMount } from "svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { Button, Select, Spinner, StatusChip, TextField, toast } from "@codeatlasdev/ui";
	import { api, errText } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";

	let aliases = $state<string[]>([]);
	let hosts = $state<string[]>([]);
	let status = $state<Record<string, string>>({});
	let pick = $state("");
	let tests = $state<Record<string, string>>({});
	let addHost = $state("");
	let addPath = $state("");
	let adding = $state(false);

	async function load() {
		aliases = await invoke<string[]>("ssh_config_hosts");
		const s = await api.settings();
		hosts = Array.isArray(s.sshHosts) ? (s.sshHosts as { alias: string }[]).map((h) => h.alias) : [];
		status = await invoke<Record<string, string>>("ssh_status");
		addHost ||= hosts[0] ?? "";
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
			tests[alias] = `${r.os} ${r.arch} · ${r.atlas ?? "atlas not installed"}`;
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
			toast.success("Remote project added");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			adding = false;
		}
	}

	const chip = (s: string | undefined) =>
		s === "connected"
			? "success"
			: s === "connecting"
				? "warning"
				: s?.startsWith("error")
					? "error"
					: "neutral";
</script>

<PageHead
	title="SSH hosts"
	description="Work on a worktree that lives on another machine. Atlas starts a small helper there and connects over SSH, so terminals, the editor, diffs and agents all run on that machine."
/>

<Group title="Your hosts">
	{#each hosts as h (h)}
		<Row label={h} description={tests[h] ?? undefined}>
			<StatusChip status={chip(status[h])} label={(status[h] ?? "idle").slice(0, 60)} />
			{#if status[h] === "connecting"}<Spinner size={11} />{/if}
			<Button size="sm" variant="outlined" onclick={() => test(h)}>Test connection</Button>
			<Button size="sm" variant="ghost" onclick={() => remove(h)}>Remove</Button>
			{#snippet below()}
				{#if status[h]?.startsWith("error")}<div class="err ca-selectable">{status[h]}</div>{/if}
			{/snippet}
		</Row>
	{:else}
		<Row
			id="ssh-hosts"
			label="No hosts yet"
			description="Pick a host from your SSH config below. Atlas never changes that file."
		/>
	{/each}
	<Row
		id={hosts.length ? "ssh-hosts" : undefined}
		label="Add a host"
		description="Choose one of the hosts already set up in your SSH config."
	>
		<div class="select">
			<Select
				size="sm"
				placeholder="Choose a host"
				value={pick}
				options={aliases.filter((a) => !hosts.includes(a)).map((a) => ({ value: a, label: a }))}
				onchange={(v) => (pick = v)}
			/>
		</div>
		<Button size="sm" disabled={!pick} onclick={add}>Add host</Button>
	</Row>
</Group>

{#if hosts.length}
	<Group
		title="Remote project"
		description="Add a project that lives on one of your hosts. The folder must contain an atlas.yaml."
	>
		<Row id="ssh-project" label="Host" description="Which machine the project is on.">
			<div class="select">
				<Select
					size="sm"
					value={addHost}
					options={hosts.map((h) => ({ value: h, label: h }))}
					onchange={(v) => (addHost = v)}
				/>
			</div>
		</Row>
		<Row label="Project folder" description="Full path on that machine." stack>
			<TextInput
				mono
				width="100%"
				label="Project folder"
				placeholder="/home/you/project"
				value={addPath}
				onchange={(v) => (addPath = v)}
			/>
		</Row>
		<Row label="Add it" description="The project then appears in your sidebar.">
			<Button size="sm" disabled={adding || !addPath.trim()} onclick={addProject}>Add project</Button>
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
