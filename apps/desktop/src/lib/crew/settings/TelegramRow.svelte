<!-- Telegram for one crew server: paste the @BotFather token, link a chat with a /start code, or remove it. -->
<script lang="ts">
	import { onMount } from "svelte";
	import { Button, TextField, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import Row from "#lib/settings/Row.svelte";
	import {
		loadTelegramStatus,
		setTelegramToken,
		startTelegramLink,
		type TelegramStatus,
	} from "#lib/crew/settings/telegram.ts";

	let { hostKey }: { hostKey: string } = $props();

	let status = $state<TelegramStatus | null>(null);
	let token = $state("");
	let busy = $state(false);
	let code = $state("");

	async function refresh() {
		status = await loadTelegramStatus(hostKey);
	}

	onMount(() => {
		void refresh().catch(() => {});
	});

	async function run(work: () => Promise<void>) {
		busy = true;
		try {
			await work();
			await refresh();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}

	const save = () =>
		run(async () => {
			await setTelegramToken(hostKey, token.trim());
			token = "";
			code = "";
		});
	const remove = () =>
		run(async () => {
			if (!confirm("Remove the Telegram bot? Linked chats stop receiving results and approvals.")) return;
			await setTelegramToken(hostKey, null);
			code = "";
		});
	const link = () =>
		run(async () => {
			code = await startTelegramLink(hostKey);
		});
</script>

<Row
	id="crew-telegram-{hostKey}"
	label="Telegram"
	description="Task results and Allow / Deny prompts in a Telegram chat, and /task to start work. Create a bot with @BotFather and paste its token."
	stack
>
	{#if status?.configured}
		<span class="state">
			Connected · {status.chats === 1 ? "1 chat linked" : `${status.chats} chats linked`}
		</span>
	{:else}
		<span class="state muted">{status ? "Not set up" : "Checking…"}</span>
	{/if}
	{#snippet below()}
		<div class="tg-form">
			<TextField
				type="password"
				autocomplete="off"
				placeholder={status?.configured ? "Replace token" : "Bot token from @BotFather"}
				bind:value={token}
				onkeydown={(e) => e.key === "Enter" && token.trim() && void save()}
				aria-label="Telegram bot token"
			/>
			<Button size="sm" onclick={save} disabled={busy || !token.trim()}>Save</Button>
			{#if status?.configured}
				<Button size="sm" variant="outlined" onclick={link} disabled={busy}>Link chat</Button>
				<Button size="sm" variant="ghost" onclick={remove} disabled={busy}>Remove</Button>
			{/if}
		</div>
		{#if code}
			<p class="tg-code" role="status">
				Send <code>/start {code}</code> to your bot in Telegram within 10 minutes.
			</p>
		{/if}
	{/snippet}
</Row>

<style>
	.tg-form {
		display: flex;
		gap: 8px;
		align-items: center;
		flex-wrap: wrap;
	}
	.tg-form > :global(:first-child) {
		flex: 1 1 220px;
	}
	.state {
		font-size: 12px;
	}
	.muted {
		color: var(--ca-color-text-muted, currentColor);
	}
	.tg-code {
		margin: 6px 0 0;
		font-size: 12px;
	}
</style>
