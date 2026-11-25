<script lang="ts">
	import { Button, SegmentedControl, Select, TextArea, TextField, toast } from "@codeatlasdev/ui";
	import { Trash2, Plus } from "@lucide/svelte";
	import { onMount } from "svelte";
	import { kbd } from "#lib/keys.ts";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import DockSettings from "./settings/DockSettings.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import TextInput from "#lib/settings/TextInput.svelte";
	import { customAgentEntry, type CustomAgentEntry } from "#lib/settings/customAgents.ts";
	import { m } from "#lib/i18n/m.ts";

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

	let loadError = $state<string | null>(null);

	onMount(async () => {
		try {
			const s = await api.settings();
			permissions = (s.agentPermissions as string) ?? "manual";
			args = (s.agentArgs as Record<string, string>) ?? {};
			custom = Array.isArray(s.customAgents) ? (s.customAgents as CustomAgentEntry[]) : [];
		} catch (e) {
			loadError = errText(e);
		}
	});

	const save = (patch: Record<string, unknown>) =>
		api.settingsPatch(patch).catch((e) => toast.error(errText(e)));
	const saveQuick = () => quick.save().catch((e) => toast.error(errText(e)));
</script>

{#if loadError}
	<LoadError what="agents" error={loadError} />
{/if}

<Group title={m.settings_agents_agent_coding_agents()}>
	{#each agents.list as a (a.id)}
		<Row
			label={a.label}
			description={a.installed
				? (a.version ?? a.path ?? m.settings_agents_agent_installed())
				: m.settings_agents_agent_not_installed()}
		/>
	{/each}
	<Row
		id="agent-default"
		label={m.settings_agents_agent_default_agent()}
		description={m.settings_agents_agent_default_description()}
	>
		<div class="select">
			<Select
				size="sm"
				value={agents.defaultAgent}
				options={[
					...agents.list.filter((a) => a.installed).map((a) => ({ value: a.id, label: a.label })),
					{ value: "blank", label: m.settings_agents_agent_default_blank() },
				]}
				onchange={(v) => agents.setDefault(v)}
			/>
		</div>
	</Row>
	<Row
		id="agent-permissions"
		label={m.settings_agents_agent_tool_approval()}
		description={m.settings_agents_agent_approval_description()}
	>
		<SegmentedControl
			aria-label={m.settings_agents_agent_tool_approval()}
			size="sm"
			value={permissions}
			items={[
				{ value: "manual", label: m.settings_agents_agent_approval_ask() },
				{ value: "yolo", label: m.settings_agents_agent_approval_auto() },
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
			label={m.settings_agents_agent_args_label({ label: a.label })}
			description={m.settings_agents_agent_args_description({ label: a.label })}
		>
			<!-- i18n-ignore: a flag example, not prose -->
			<TextInput
				mono
				label={m.settings_agents_agent_args_label({ label: a.label })}
				placeholder="--model opus"
				value={args[a.id] ?? ""}
				onchange={(v) => {
					args[a.id] = v;
					save({ agentArgs: { ...args } });
				}}
			/>
		</Row>
	{/each}
</Group>

<DockSettings />

<Group
	title={m.settings_agents_agent_custom_agents()}
	description={m.settings_agents_agent_custom_description({ key: kbd("cmd-shift-a") })}
>
	{#each custom as c (c.id)}
		<Row label={c.label} description={[c.command, ...c.args].join(" ")}>
			<button
				class="rm"
				aria-label={m.settings_agents_agent_remove_label({ label: c.label })}
				onclick={() => saveCustom(custom.filter((x) => x !== c))}
			>
				<Trash2 size={14} />
			</button>
		</Row>
	{/each}
	<div data-setting="agent-custom" class="custom">
		<!-- i18n-ignore: a product name used as an example -->
		<TextField label={m.settings_agents_agent_name()} placeholder="OpenCode" bind:value={form.name} />
		<!-- i18n-ignore: a command name used as an example -->
		<TextField label={m.settings_agents_agent_command()} placeholder="opencode" bind:value={form.command} />
		<!-- i18n-ignore: a flag example, not prose -->
		<TextField
			label={m.settings_agents_agent_arguments()}
			placeholder="--model gpt-5"
			bind:value={form.args}
		/>
		<!-- i18n-ignore: a flag with a placeholder, not prose -->
		<TextField
			label={m.settings_agents_agent_resume_arguments()}
			placeholder="--session {'{session}'}"
			bind:value={form.resume}
		/>
		<TextArea
			label={m.settings_agents_agent_environment()}
			placeholder={m.settings_agents_agent_environment_placeholder()}
			rows={2}
			bind:value={form.env}
		/>
		{#if formError}<p class="err" role="alert">{formError}</p>{/if}
		<div>
			<Button size="sm" onclick={addCustom}><Plus size={14} /> {m.settings_agents_agent_add_agent()}</Button>
		</div>
	</div>
</Group>

<Group
	title={m.settings_agents_agent_quick_commands()}
	description={m.settings_agents_agent_quick_description()}
>
	{#each quick.list as q (q.id)}
		<div class="quick" data-setting={q === quick.list[0] ? "quick" : undefined}>
			<TextField label={m.settings_agents_agent_quick_name()} bind:value={q.label} onchange={saveQuick} />
			<TextField
				label={q.kind === "prompt"
					? m.settings_agents_agent_quick_prompt()
					: m.settings_agents_agent_quick_command()}
				bind:value={q.command}
				onchange={saveQuick}
			/>
			<Select
				label={m.settings_agents_agent_quick_kind()}
				value={q.kind}
				options={[
					{ value: "terminal", label: m.settings_agents_agent_quick_kind_terminal() },
					{ value: "prompt", label: m.capture_quick_page_send_to_agent() },
				]}
				onchange={(v) => {
					q.kind = v as typeof q.kind;
					saveQuick();
				}}
			/>
			<Select
				label={m.settings_agents_agent_quick_scope()}
				value={q.scope}
				options={[
					{ value: "global", label: m.settings_agents_agent_quick_scope_all() },
					...store.projects.filter((p) => !p.parent).map((p) => ({ value: p.id, label: p.name })),
				]}
				onchange={(v) => {
					q.scope = v;
					saveQuick();
				}}
			/>
			<button
				class="rm"
				aria-label={m.settings_agents_agent_quick_remove()}
				onclick={() => {
					quick.list = quick.list.filter((x) => x.id !== q.id);
					saveQuick();
				}}
			>
				<Trash2 size={14} />
			</button>
		</div>
	{/each}
	<Row
		id={quick.list.length ? undefined : "quick"}
		label={m.settings_agents_agent_quick_add_label()}
		description={quick.list.length ? undefined : m.settings_agents_agent_quick_add_hint()}
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
			{m.settings_agents_agent_quick_add_button()}
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
	.custom :global(.ca-ta),
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
