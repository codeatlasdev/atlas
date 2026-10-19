<script lang="ts">
	import { Button, SegmentedControl, Select, TextArea, TextField, toast } from "@codeatlasdev/ui";
	import { Trash2, Plus } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";
	import { customAgentEntry, type CustomAgentEntry } from "#lib/settings/customAgents.ts";

	let permissions = $state("manual");
	let args = $state<Record<string, string>>({});
	let custom = $state<CustomAgentEntry[]>([]);
	const empty = { name: "", command: "", args: "", resume: "", env: "" };
	let form = $state({ ...empty });
	let formError = $state("");

	async function saveCustom(next: CustomAgentEntry[]) {
		custom = next;
		await api
			.settingsPatch({ customAgents: next.length ? next : null })
			.catch((e) => toast.error(errText(e)));
		await agents.refresh();
	}

	async function addCustom() {
		const r = customAgentEntry(
			form,
			agents.list.map((a) => a.id),
		);
		if (!r.ok) {
			formError = r.error;
			return;
		}
		formError = "";
		form = { ...empty };
		await saveCustom([...custom, r.entry]);
	}

	onMount(async () => {
		const s = await api.settings();
		permissions = (s.agentPermissions as string) ?? "manual";
		args = (s.agentArgs as Record<string, string>) ?? {};
		custom = Array.isArray(s.customAgents) ? (s.customAgents as CustomAgentEntry[]) : [];
	});

	const save = (patch: Record<string, unknown>) =>
		api.settingsPatch(patch).catch((e) => toast.error(errText(e)));
	const saveQuick = () => quick.save().catch((e) => toast.error(errText(e)));
</script>

<PageHead
	title="Agents"
	description="Coding agents run in terminals inside your worktrees. Choose which one starts by default, how much it may do on its own, and the quick commands in the tab strip."
/>

<Group title="Coding agents">
	{#each agents.list as a (a.id)}
		<Row
			label={a.label}
			description={a.installed ? (a.version ?? a.path ?? "Installed") : "Not installed on this computer"}
		/>
	{/each}
	<Row
		id="agent-default"
		label="Default agent"
		description="What a new agent tab starts. Blank terminal opens a plain shell."
	>
		<div class="select">
			<Select
				size="sm"
				value={agents.defaultAgent}
				options={[
					...agents.list.filter((a) => a.installed).map((a) => ({ value: a.id, label: a.label })),
					{ value: "blank", label: "Blank terminal" },
				]}
				onchange={(v) => agents.setDefault(v)}
			/>
		</div>
	</Row>
	<Row
		id="agent-permissions"
		label="Tool approval"
		description="Ask first: the agent waits for your OK before running commands or editing files. Auto-approve: it never asks, so use it only in worktrees you can throw away."
	>
		<SegmentedControl
			aria-label="Tool approval"
			size="sm"
			value={permissions}
			items={[
				{ value: "manual", label: "Ask first" },
				{ value: "yolo", label: "Auto-approve" },
			]}
			onchange={(v) => {
				permissions = v;
				save({ agentPermissions: v });
			}}
		/>
	</Row>
	{#each agents.list.filter((a) => a.installed) as a (a.id)}
		<Row
			id={a.id === agents.list.find((x) => x.installed)?.id ? "agent-args" : undefined}
			label="Extra arguments for {a.label}"
			description="Flags added every time {a.label} starts, for example --model opus."
		>
			<TextInput
				mono
				label="Extra arguments for {a.label}"
				placeholder="--model opus"
				value={args[a.id] ?? ""}
				onchange={(v) => {
					args[a.id] = v;
					save({ agentArgs: { ...args } });
				}}
			/>
		</Row>
	{/each}
	{#if agents.ompIntegration === "missing"}
		<Row
			id="agent-omp"
			label="omp integration"
			description="Lets omp tell Atlas when it is working, waiting for you or done, and use Atlas tools."
		>
			<Button size="sm" onclick={() => agents.enableOmp()}>Turn on</Button>
		</Row>
	{/if}
</Group>

<Group
	title="Custom agents"
	description="Any agent CLI (OpenCode, Gemini CLI, Amp…). It starts from the + menu and ⌘⇧A like the built-in ones. Without Atlas hooks its tab shows working while the terminal prints and idle after a few quiet seconds; to report real states it can run `atlas hook` (see docs/agents.md)."
>
	{#each custom as c (c.id)}
		<Row label={c.label} description={[c.command, ...c.args].join(" ")}>
			<button
				class="rm"
				aria-label="Remove {c.label}"
				onclick={() => saveCustom(custom.filter((x) => x !== c))}><Trash2 size={14} /></button
			>
		</Row>
	{/each}
	<div data-setting="agent-custom" class="custom">
		<TextField label="Name" placeholder="OpenCode" bind:value={form.name} />
		<TextField label="Command" placeholder="opencode" bind:value={form.command} />
		<TextField label="Arguments" placeholder="--model gpt-5" bind:value={form.args} />
		<TextField label="Resume arguments" placeholder="--session {'{session}'}" bind:value={form.resume} />
		<TextArea label="Environment" placeholder="KEY=value, one per line" rows={2} bind:value={form.env} />
		{#if formError}<p class="err" role="alert">{formError}</p>{/if}
		<div><Button size="sm" onclick={addCustom}><Plus size={14} /> Add agent</Button></div>
	</div>
</Group>

<Group
	title="Quick commands"
	description="One-click buttons in the tab strip. Each one runs a command in a terminal, or sends a prompt to the agent."
>
	{#each quick.list as q (q.id)}
		<div class="quick" data-setting={q === quick.list[0] ? "quick" : undefined}>
			<TextField label="Button name" bind:value={q.label} onchange={saveQuick} />
			<TextField
				label={q.kind === "prompt" ? "Prompt to send" : "Command to run"}
				bind:value={q.command}
				onchange={saveQuick}
			/>
			<Select
				label="What it does"
				value={q.kind}
				options={[
					{ value: "terminal", label: "Run in terminal" },
					{ value: "prompt", label: "Send to agent" },
				]}
				onchange={(v) => {
					q.kind = v as typeof q.kind;
					saveQuick();
				}}
			/>
			<Select
				label="Show in"
				value={q.scope}
				options={[
					{ value: "global", label: "All projects" },
					...store.projects.filter((p) => !p.parent).map((p) => ({ value: p.id, label: p.name })),
				]}
				onchange={(v) => {
					q.scope = v;
					saveQuick();
				}}
			/>
			<button
				class="rm"
				aria-label="Remove quick command"
				onclick={() => {
					quick.list = quick.list.filter((x) => x.id !== q.id);
					saveQuick();
				}}><Trash2 size={14} /></button
			>
		</div>
	{/each}
	<Row
		id={quick.list.length ? undefined : "quick"}
		label="Add a quick command"
		description={quick.list.length ? undefined : "Example: Run tests, or Review my changes."}
	>
		<Button
			size="sm"
			variant="glass"
			onclick={() => {
				quick.add();
				saveQuick();
			}}
		>
			{#snippet leading()}<Plus size={13} />{/snippet}
			Add command
		</Button>
	</Row>
</Group>

<style>
	.select {
		width: 180px;
	}
	.quick {
		display: grid;
		grid-template-columns: 1fr 2fr 1.2fr 1.2fr 28px;
		gap: 8px;
		align-items: end;
		padding: 12px 14px;
	}
	.rm {
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
		height: 30px;
	}
	.custom {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
		padding: 12px 14px;
	}
	.custom :global(.ca-textarea),
	.custom .err,
	.custom > div:last-child {
		grid-column: 1 / -1;
	}
	.err {
		margin: 0;
		font-size: 12px;
		color: var(--ca-semantic-error);
	}
</style>
