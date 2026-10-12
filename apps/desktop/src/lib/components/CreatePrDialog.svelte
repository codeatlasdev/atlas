<script lang="ts">
	import { Button, Checkbox, Dialog, Spinner, TextArea, TextField, toast } from "@codeatlasdev/ui";
	import { Sparkles } from "@lucide/svelte";
	import { git, errText } from "#lib/api.ts";
	import { host } from "#lib/host.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";

	let title = $state("");
	let body = $state("");
	let base = $state("");
	let draft = $state(false);
	let busy = $state<"ai" | "create" | null>(null);

	let open = $derived(!!pr.createFor);
	let id = $derived(pr.createFor ?? "");

	$effect(() => {
		if (!pr.createFor) return;
		const wid = pr.createFor;
		title = body = "";
		draft = false;
		git
			.status(wid)
			.then(
				(s) => (base = s.base.replace(/^origin\//, "") === "HEAD" ? "" : s.base.replace(/^origin\//, "")),
			);
		void pr.checkAuth(wid);
	});

	async function generate() {
		busy = "ai";
		try {
			const b = base ? `origin/${base}` : "HEAD~1";
			const log = await git.run(id, ["log", "--oneline", `${b}..HEAD`]);
			const diff = await git.run(id, ["diff", "--no-color", `${b}...HEAD`]);
			const r = await host.exec(
				ws.worktree(id)!.path,
				[
					"omp",
					"-p",
					"--no-session",
					`Write a GitHub pull request for these commits. First line: the title. Then a blank line, then a markdown body.\n\nCommits:\n${log.stdout}\n\nDiff:\n${diff.stdout.slice(0, 60_000)}`,
				],
				{ timeout_ms: 90_000 },
			);
			if (r.code !== 0) throw r.stderr || "omp failed";
			const [t, ...rest] = r.stdout.trim().split("\n");
			title = t.replace(/^#+\s*/, "");
			body = rest.join("\n").trim();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = null;
		}
	}

	async function create() {
		if (!title.trim()) return;
		busy = "create";
		await pr.create(id, { title, body, base, draft });
		busy = null;
		pr.createFor = null;
	}

	function signIn() {
		pr.createFor = null;
		void ws.newTerminal(id, { argv: ["gh", "auth", "login"], label: "gh auth" });
	}
</script>

<Dialog {open} onOpenChange={(o) => !o && (pr.createFor = null)} title="Create pull request" size="lg">
	{#if pr.auth === "missing"}
		<p>GitHub CLI is not signed in.</p>
		<Button onclick={signIn}>Sign in with gh</Button>
	{:else}
		<div class="form">
			<TextField label="Title" bind:value={title} />
			<TextArea label="Description" bind:value={body} rows={10} />
			<TextField label="Base branch" bind:value={base} placeholder="repository default" />
			<Checkbox bind:checked={draft} label="Draft" />
		</div>
	{/if}
	{#snippet actions()}
		<Button variant="ghost" disabled={!!busy} onclick={generate}>
			{#snippet leading()}{#if busy === "ai"}<Spinner size={12} />{:else}<Sparkles size={12} />{/if}{/snippet}
			Generate with AI
		</Button>
		<Button variant="ghost" onclick={() => (pr.createFor = null)}>Cancel</Button>
		<Button disabled={!title.trim() || !!busy || pr.auth === "missing"} onclick={create}>
			{#snippet leading()}{#if busy === "create"}<Spinner size={12} />{/if}{/snippet}
			Create
		</Button>
	{/snippet}
</Dialog>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
</style>
