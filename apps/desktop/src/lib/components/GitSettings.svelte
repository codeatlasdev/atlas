<script lang="ts">
	import { Select, toast } from "@codeatlasdev/ui";
	import { onMount } from "svelte";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { commitMessages } from "#lib/stores/commitMessage.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import LoadError from "#lib/settings/LoadError.svelte";

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
	<LoadError what="your Git settings" error={loadError} />
{/if}

<Group title="Commit messages">
	<Row
		id="git-commit-agent"
		label="Generate commit messages with"
		description="The staged diff (all changes when nothing is staged) goes only to the agent you pick here, run on this computer with no tools, plus your recent commit subjects and the commit rules of CONTRIBUTING.md and AGENTS.md. Files that look like secrets (.env, keys, certificates) are never sent; lockfiles and binaries are sent as names only. Choose Off to hide the button."
	>
		<div class="select">
			<Select
				size="sm"
				value={choice}
				options={[
					{ value: "auto", label: "Automatic" },
					...agents.list.filter((a) => a.installed).map((a) => ({ value: a.id, label: a.label })),
					{ value: "off", label: "Off" },
				]}
				onchange={save}
			/>
		</div>
		{#snippet below()}
			{#if !agents.list.some((a) => a.installed)}
				<p class="hint">
					No coding agent is installed on this computer, so there is nothing to write messages yet. Install
					omp, Claude Code or Codex and pick it here.
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
