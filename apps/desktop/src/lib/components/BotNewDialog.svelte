<script lang="ts">
	import BotEditor from "#lib/bots/BotEditor.svelte";
	import { randomLook, type BotLook } from "#lib/bots/look.ts";
	import { Button, Dialog, Select, TextField, TextArea, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import type { AgentIntegration } from "#lib/stores/agentIntegration.ts";

	interface Props {
		open: boolean;
		onCreated?: (hostKey: string, botId: string) => void;
	}

	let { open = $bindable(), onCreated }: Props = $props();

	let name = $state("");
	let role = $state("");
	let selectedAgent = $state("");
	let selectedHostKey = $state("");
	let approval = $state<"ask" | "auto">("ask");
	let creating = $state(false);
	let look = $state<BotLook>(randomLook());
	let integrations = $state<AgentIntegration[]>([]);

	let remoteHosts = $derived(bots.hosts.filter((h) => h.key !== ""));

	// Default selectedHostKey to first remote host when dialog opens
	$effect(() => {
		if (!open) return;
		name = "";
		role = "";
		approval = "ask";
		selectedHostKey = remoteHosts[0]?.key ?? "";
	});

	// Reload integrations whenever selectedHostKey changes (or dialog opens)
	$effect(() => {
		if (!open) return;
		const key = selectedHostKey;
		integrations = [];
		selectedAgent = "";
		bots
			.getHostIntegrations(key)
			.then((list) => {
				integrations = list;
				const first = list.find((i) => i.state === "ok");
				selectedAgent = first?.id ?? "";
			})
			.catch(() => {});
	});

	// omp forces auto-approval
	$effect(() => {
		if (selectedAgent === "omp") approval = "auto";
	});

	let okAgents = $derived(integrations.filter((i) => i.state === "ok"));
	let agentOptions = $derived(okAgents.map((i) => ({ value: i.id, label: i.label })));
	let hostOptions = $derived(remoteHosts.map((h) => ({ value: h.key, label: h.name })));
	// Show server select only when there are >1 remote hosts
	let showServerSelect = $derived(remoteHosts.length > 1);

	const approvalOptions = [
		{ value: "ask", label: "Ask before acting" },
		{ value: "auto", label: "Automatic" },
	];

	async function create() {
		if (!name.trim() || !selectedAgent || creating) return;
		creating = true;
		try {
			const id = await bots.createBot(selectedHostKey, {
				name: name.trim(),
				role: role.trim(),
				agent: selectedAgent,
				approval,
				look,
			});
			open = false;
			onCreated?.(selectedHostKey, id);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			creating = false;
		}
	}
</script>

<Dialog bind:open title="New bot" size="lg">
	{#if okAgents.length === 0 && integrations.length > 0}
		<p class="empty">
			No agents are set up on this server. Run <code>atlas bot login &lt;server&gt; &lt;agent&gt;</code> to sign
			in.
		</p>
	{:else}
		<form
			class="form"
			onsubmit={(e) => {
				e.preventDefault();
				void create();
			}}
		>
			{#if showServerSelect}
				<Select
					label="Server"
					value={selectedHostKey}
					options={hostOptions}
					onchange={(v) => (selectedHostKey = v)}
				/>
			{/if}
			<BotEditor bind:look seed={name || "new"} />
			<TextField label="Name" bind:value={name} placeholder="My bot" />
			<TextArea label="Role" bind:value={role} placeholder="Reviews PRs, checks test results…" rows={3} />
			{#if agentOptions.length > 0}
				<Select
					label="Agent"
					value={selectedAgent}
					options={agentOptions}
					onchange={(v) => (selectedAgent = v)}
				/>
			{:else if integrations.length === 0}
				<p class="empty">
					No agents are set up on this server. Run <code>atlas bot login &lt;server&gt; &lt;agent&gt;</code> to
					sign in.
				</p>
			{/if}
			<Select
				label="Approval"
				value={approval}
				options={approvalOptions}
				onchange={(v) => (approval = v as "ask" | "auto")}
				disabled={selectedAgent === "omp"}
			/>
			{#if selectedAgent === "omp"}
				<p class="hint">omp bots run without approval prompts</p>
			{/if}
			<!-- hidden submit so Enter in text fields triggers create -->
			<button type="submit" hidden aria-hidden="true"></button>
		</form>
	{/if}
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
		<Button disabled={!name.trim() || !selectedAgent || creating || okAgents.length === 0} onclick={create}>
			{creating ? "Creating…" : "Create"}
		</Button>
	{/snippet}
</Dialog>

<style>
	.form {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.empty {
		color: var(--text-2);
		font-size: 13px;
		line-height: 1.5;
		margin: 0;
	}

	.hint {
		color: var(--text-2);
		font-size: 12px;
		margin: -4px 0 0;
	}

	code {
		font-family: var(--font-mono, monospace);
		background: var(--surface-1);
		border-radius: 3px;
		padding: 1px 4px;
	}
</style>
