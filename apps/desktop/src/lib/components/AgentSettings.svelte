<script lang="ts">
	import { Button, SegmentedControl, Select, Switch, TextField, toast } from "@codeatlasdev/ui";
	import { Trash2, Plus } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { browserPrefs } from "#lib/stores/browser.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";

	let permissions = $state("manual");
	let args = $state<Record<string, string>>({});

	onMount(async () => {
		const s = await api.settings();
		permissions = (s.agentPermissions as string) ?? "manual";
		args = (s.agentArgs as Record<string, string>) ?? {};
	});

	const save = (patch: Record<string, unknown>) =>
		api.settingsPatch(patch).catch((e) => toast.error(errText(e)));
	const saveQuick = () => quick.save().catch((e) => toast.error(errText(e)));
</script>

<PageHead
	title="Agents"
	description="Coding agents run in terminals inside your worktrees. Choose which one starts by default, how much it may do on its own, and how Atlas keeps you informed."
/>

<Group title="Coding agents">
	{#each agents.list as a (a.id)}
		<Row
			label={a.label}
			description={a.installed ? (a.version ?? a.path ?? "Installed") : "Not installed on this Mac"}
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

<div class="gap"></div>

<Group title="Links">
	<Row
		id="browser"
		label="Open links in"
		description="Where links from services, ports and terminals open. Hold ⌘ while clicking to use the other browser."
	>
		<SegmentedControl
			aria-label="Open links in"
			size="sm"
			value={browserPrefs.openIn}
			items={[
				{ value: "atlas", label: "Atlas browser" },
				{ value: "system", label: "System browser" },
			]}
			onchange={(v) => browserPrefs.set(v as "atlas" | "system")}
		/>
	</Row>
</Group>

<div class="gap"></div>

<Group title="Notifications" description="Atlas tells you when an agent finishes or needs you.">
	<Row
		id="notify"
		label="Show notifications"
		description="A system notification when an agent is done or is waiting for an answer."
	>
		<Switch
			aria-label="Show notifications"
			bind:checked={agents.notify.enabled}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row label="Play a sound" description="Add a short sound to each notification.">
		<Switch
			aria-label="Play a sound"
			bind:checked={agents.notify.sound}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row
		label="Stay quiet while I'm watching"
		description="Skip the notification when that worktree is the one on your screen."
	>
		<Switch
			aria-label="Stay quiet while I'm watching"
			bind:checked={agents.notify.suppressWhenFocused}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
</Group>

<div class="gap"></div>

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
	.gap {
		height: 4px;
	}
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
</style>
