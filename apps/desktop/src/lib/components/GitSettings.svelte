<script lang="ts">
	import { Select, toast } from "@codeatlasdev/ui";
	import { onMount } from "svelte";
	import { api, errText } from "#lib/api.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { commitMessages } from "#lib/stores/commitMessage.svelte.ts";
	import Group from "#lib/settings/Group.svelte";
	import PageHead from "#lib/settings/PageHead.svelte";
	import Row from "#lib/settings/Row.svelte";

	/** `auto` = your default agent when it is installed, else the first installed one. */
	let choice = $state("auto");

	onMount(async () => {
		const s = await api.settings();
		choice = (s.gitCommitAgent as string) ?? "auto";
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

<PageHead
	title="Git"
	description="How Atlas helps with commits. Nothing is committed for you: a generated message lands in the commit box, fully editable."
/>

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
	</Row>
</Group>

<style>
	.select {
		min-width: 170px;
	}
</style>
