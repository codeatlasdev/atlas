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
	import { open as pickFolder } from "@tauri-apps/plugin-dialog";
	import { api, errText } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { worktrees } from "#lib/stores/worktrees.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { host } from "#lib/host.ts";

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
		{ value: "blank", label: "Blank terminal" },
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
		const dir = await pickFolder({ directory: true, title: "Add atlas project" });
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

<Dialog bind:open={worktrees.newDialog.open} title="New worktree" size="md">
	<form
		class="form"
		onsubmit={(e) => {
			e.preventDefault();
			void create();
		}}
	>
		<Combobox
			label="Project"
			value={repoId}
			options={[
				...repos.map((r) => ({ value: r.id, label: r.name })),
				{ value: ADD, label: "Add a new project…" },
			]}
			onchange={(v) => typeof v === "string" && pickRepo(v)}
		/>
		<TextField
			label="Name"
			bind:value={name}
			placeholder="Leave blank for a random name"
			description="A GitHub PR or issue URL works too."
		/>
		<Select label="Agent" value={agent} options={agentOptions} onchange={(v) => (agent = v)} />
		{#if agent !== "blank"}
			<TextArea
				label="Prompt"
				bind:value={prompt}
				placeholder="Optional first message to the agent"
				rows={3}
			/>
		{/if}
		<Collapsible title="Advanced">
			<div class="adv">
				<TextField label="Branch name" bind:value={branch} placeholder="Derived from the name" />
				<Combobox
					label="Base ref"
					value={base}
					placeholder="origin/<default branch>"
					options={refs.map((r) => ({ value: r, label: r }))}
					onchange={(v) => typeof v === "string" && (base = v)}
				/>
				<Checkbox bind:checked={runSetup} label="Run setup script (worktree.setup)" />
			</div>
		</Collapsible>
		<button type="submit" hidden aria-hidden="true"></button>
	</form>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (worktrees.newDialog.open = false)}>Cancel</Button>
		<Button disabled={!repoId || repoId === ADD || busy} onclick={create}>Create</Button>
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
