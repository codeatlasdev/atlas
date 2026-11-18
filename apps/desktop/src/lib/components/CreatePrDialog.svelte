<script lang="ts">
	import { Button, Checkbox, Dialog, Spinner, TextArea, TextField } from "@codeatlasdev/ui";
	import { Sparkles } from "@lucide/svelte";
	import { api, git, errText } from "#lib/api.ts";
	import { errorToast } from "#lib/gitToast.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import { host } from "#lib/host.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

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
					// i18n-ignore: the prompt an agent reads
					`Write a GitHub pull request for these commits. First line: the title. Then a blank line, then a markdown body.\n\nCommits:\n${log.stdout}\n\nDiff:\n${diff.stdout.slice(0, 60_000)}`,
				],
				{ timeout_ms: 90_000 },
			);
			if (r.code !== 0) throw r.stderr || "omp failed";
			const [t, ...rest] = r.stdout.trim().split("\n");
			title = t.replace(/^#+\s*/, "");
			body = rest.join("\n").trim();
		} catch (e) {
			errorToast(m.git_create_pr_dialog_generate_failed(), errText(e));
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
		pr.signIn(id);
	}
</script>

<Dialog {open} onOpenChange={(o) => !o && (pr.createFor = null)} title={m.git_create_pr_dialog_create_pull_request()} size="lg">
	{#if pr.ghUnusable}
		{#if pr.auth === "not-installed"}
			<p><Rich parts={m.git_create_pr_dialog_cli_not_installed.parts()} tags={{ code: "code" }} /></p>
			<Button onclick={() => api.openUrl("https://cli.github.com")}>{m.git_create_pr_dialog_install_github_cli()}</Button>
		{:else}
			<p>{m.git_create_pr_dialog_github_cli_is_not_signed()}</p>
			<Button onclick={signIn}>{m.git_create_pr_dialog_sign_in_with_gh()}</Button>
		{/if}
	{:else}
		<div class="form">
			<TextField label={m.git_create_pr_dialog_title()} bind:value={title} />
			<TextArea label={m.git_create_pr_dialog_description()} bind:value={body} rows={10} />
			<TextField label={m.git_create_pr_dialog_base_branch()} bind:value={base} placeholder={m.git_create_pr_dialog_repository_default()} />
			<Checkbox bind:checked={draft} label={m.git_create_pr_dialog_draft()} />
		</div>
	{/if}
	{#snippet actions()}
		<Button variant="ghost" disabled={!!busy} onclick={generate}>
			{#snippet leading()}{#if busy === "ai"}<Spinner size={12} />{:else}<Sparkles size={12} />{/if}{/snippet}
			{m.git_create_pr_dialog_generate_with_ai()}
		</Button>
		<Button variant="ghost" onclick={() => (pr.createFor = null)}>{m.common_action_cancel()}</Button>
		<Button disabled={!title.trim() || !!busy || pr.ghUnusable} onclick={create}>
			{#snippet leading()}{#if busy === "create"}<Spinner size={12} />{/if}{/snippet}
			{m.git_create_pr_dialog_create()}
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
