<script lang="ts">
	import {
		Button,
		Checkbox,
		Collapsible,
		Combobox,
		Dialog,
		Select,
		TextField,
		TextArea,
		toast,
	} from "@codeatlasdev/ui";
	import { api, errText } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { host } from "#lib/host.ts";
	import { m } from "#lib/i18n/m.ts";

	const ADD = "__add__";
	const PR_URL = /^https:\/\/github\.com\/[^/]+\/[^/]+\/pull\/(\d+)/;
	const ISSUE_URL = /^https:\/\/github\.com\/[^/]+\/[^/]+\/issues\/(\d+)/;

	let repoId = $state("");
	let name = $state("");
	let agent = $state("omp");
	let prompt = $state("");
	let branch = $state("");
	let base = $state("");
	let runSetup = $state(true);
	let refs = $state<string[]>([]);
	let busy = $state(false);

	let repos = $derived(store.sorted.filter((p) => !p.parent));
	let agentOptions = $derived([
		...agents.list.filter((a) => a.installed).map((a) => ({ value: a.id, label: a.label })),
		{ value: "blank", label: m.git_new_worktree_dialog_blank_terminal() },
	]);

	$effect(() => {
		if (!worktrees.newDialog.open) return;
		repoId = worktrees.newDialog.repoId ?? store.current?.parent ?? store.current?.id ?? repos[0]?.id ?? "";
		name = prompt = branch = base = "";
		runSetup = true;
		agent = agents.defaultAgent;
	});

	$effect(() => {
		const id = repoId;
		refs = [];
		if (id && id !== ADD && worktrees.newDialog.open)
			api
				.gitRefs(id)
				.then((r) => (refs = r))
				.catch(() => {});
	});

	async function pickRepo(v: string) {
		if (v !== ADD) {
			repoId = v;
			return;
		}
		const dir = await api.pickPath({ directory: true, title: m.git_new_worktree_dialog_add_project_title() });
		if (typeof dir !== "string") return;
		try {
			const p = await api.add(dir);
			await store.refresh();
			repoId = p.id;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function create() {
		if (!repoId || repoId === ADD || busy) return;
		busy = true;
		let pr: number | undefined;
		let n = name.trim();
		let p = prompt.trim();
		const prm = n.match(PR_URL);
		const ism = n.match(ISSUE_URL);
		if (prm) {
			pr = Number(prm[1]);
			n = "";
		} else if (ism) {
			const root = store.projects.find((x) => x.id === repoId)?.root ?? "";
			const r = await host
				.exec(root, ["gh", "issue", "view", ism[1], "--json", "title,body"])
				.catch(() => null);
			if (r?.code === 0) {
				const issue = JSON.parse(r.stdout) as { title: string; body: string };
				n = issue.title;
				if (!p) p = `Work on GitHub issue #${ism[1]}: ${issue.title}\n\n${issue.body}`;
			} else n = `issue-${ism[1]}`;
		}
		worktrees.newDialog.open = false;
		busy = false;
		await worktrees.create({
			repoId,
			name: n,
			pr,
			branch: branch.trim() || undefined,
			base: base.trim() || undefined,
			runSetup,
			agent,
			prompt: p || undefined,
		});
	}
</script>

<Dialog bind:open={worktrees.newDialog.open} title={m.git_new_worktree_dialog_new_worktree()} size="md">
	<form
		class="form"
		onsubmit={(e) => {
			e.preventDefault();
			void create();
		}}
	>
		<Combobox
			label={m.git_new_worktree_dialog_project()}
			value={repoId}
			options={[
				...repos.map((r) => ({ value: r.id, label: r.name })),
				{ value: ADD, label: m.git_new_worktree_dialog_add_new_project() },
			]}
			onchange={(v) => typeof v === "string" && pickRepo(v)}
		/>
		<TextField
			label={m.git_new_worktree_dialog_name()}
			bind:value={name}
			placeholder={m.git_new_worktree_dialog_leave_blank_for_a_random()}
			description={m.git_new_worktree_dialog_a_github_pr_or_issue()}
		/>
		<Select
			label={m.git_new_worktree_dialog_agent()}
			value={agent}
			options={agentOptions}
			onchange={(v) => (agent = v)}
		/>
		{#if agent !== "blank"}
			<TextArea
				label={m.git_new_worktree_dialog_prompt()}
				bind:value={prompt}
				placeholder={m.git_new_worktree_dialog_optional_first_message_to_the()}
				rows={3}
			/>
		{/if}
		<Collapsible title={m.git_new_worktree_dialog_advanced()}>
			<div class="adv">
				<TextField
					label={m.git_new_worktree_dialog_branch_name()}
					bind:value={branch}
					placeholder={m.git_new_worktree_dialog_derived_from_the_name()}
				/>
				<Combobox
					label={m.git_new_worktree_dialog_base_ref()}
					value={base}
					placeholder={m.git_new_worktree_dialog_origin_default_branch()}
					options={refs.map((r) => ({ value: r, label: r }))}
					onchange={(v) => typeof v === "string" && (base = v)}
				/>
				<Checkbox
					bind:checked={runSetup}
					label={m.git_new_worktree_dialog_run_setup_script_worktree_setup()}
				/>
			</div>
		</Collapsible>
		<button type="submit" hidden aria-hidden="true"></button>
	</form>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (worktrees.newDialog.open = false)}
			>{m.common_action_cancel()}</Button
		>
		<Button disabled={!repoId || repoId === ADD || busy} onclick={create}
			>{m.git_create_pr_dialog_create()}</Button
		>
	{/snippet}
</Dialog>

<style>
	.form,
	.adv {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.adv {
		padding-top: 8px;
	}
</style>
