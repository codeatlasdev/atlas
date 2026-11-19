<script lang="ts">
	import { Select, toast } from "@codeatlasdev/ui";
	import { onMount } from "svelte";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { BUILTIN_AGENTS, canWriteCommits } from "#lib/agentRegistry.ts";
	import { commitMessages } from "#lib/stores/commitMessage.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";
	import { m } from "#lib/i18n/m.ts";
	import { list } from "#lib/i18n/fmt.ts";

	/** The installed agents that can write a message (a one-shot run in `agents.json`: not OpenCode, not custom agents). */
	const writers = $derived(agents.list.filter((a) => a.installed && canWriteCommits(a.id)));
	const writerNames = $derived(
		list(
			BUILTIN_AGENTS.filter((a) => canWriteCommits(a.id)).map((a) => a.label),
			"disjunction",
		),
	);

	/** `auto` = your default agent when it is installed, else the first installed one. */
	let choice = $state("auto");

	let loadError = $state<string | null>(null);

	onMount(async () => {
		try {
			const s = await api.settings();
			choice = (s.gitCommitAgent as string) ?? "auto";
		} catch (e) {
			loadError = errText(e);
		}
	});

	async function save(v: string) {
		choice = v;
		try {
			await api.settingsPatch({ gitCommitAgent: v });
			await commitMessages.loadProvider();
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

{#if loadError}
	<LoadError what="git" error={loadError} />
{/if}

<Group title={m.settings_general_git_commit_messages()}>
	<Row
		id="git-commit-agent"
		label={m.settings_general_git_generate_commit_messages_with()}
		description={m.settings_general_git_the_staged_diff_all_changes()}
	>
		<div class="select">
			<Select
				size="sm"
				value={choice}
				options={[
					{ value: "auto", label: m.settings_general_git_option_automatic() },
					...writers.map((a) => ({ value: a.id, label: a.label })),
					{ value: "off", label: m.settings_capture_timer_off() },
				]}
				onchange={save}
			/>
		</div>
		{#snippet below()}
			{#if writers.length === 0}
				<p class="hint">
					{m.settings_general_git_none_of_writernames_is_installed({ writerNames })}
				</p>
			{/if}
		{/snippet}
	</Row>
</Group>

<style>
	.select {
		min-width: 170px;
	}
	.hint {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
