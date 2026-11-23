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
	import { m } from "#lib/i18n/m.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";

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
			const ok = await confirmDialog(m.crew_settings_telegram_row_remove_confirm(), {
				confirmLabel: m.common_action_remove(),
				destructive: true,
			});
			if (!ok) return;
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
	label={m.crew_settings_telegram_row_telegram()}
	description={m.crew_settings_telegram_row_task_results_and_allow_deny()}
	stack
>
	{#if status?.configured}
		<span class="state">{m.crew_settings_telegram_row_connected({ count: status.chats })}</span>
	{:else}
		<span class="state muted"
			>{status ? m.crew_settings_telegram_row_not_set_up() : m.crew_settings_browsers_row_checking()}</span
		>
	{/if}
	{#snippet below()}
		<div class="tg-form">
			<TextField
				type="password"
				autocomplete="off"
				placeholder={status?.configured
					? m.crew_settings_telegram_row_replace_token()
					: m.crew_settings_telegram_row_bot_token_hint()}
				bind:value={token}
				onkeydown={(e) => e.key === "Enter" && token.trim() && void save()}
				aria-label={m.crew_settings_telegram_row_telegram_bot_token()}
			/>
			<Button size="sm" onclick={save} disabled={busy || !token.trim()}>{m.common_action_save()}</Button>
			{#if status?.configured}
				<Button size="sm" variant="outlined" onclick={link} disabled={busy}
					>{m.crew_settings_telegram_row_link_chat()}</Button
				>
				<Button size="sm" variant="ghost" onclick={remove} disabled={busy}>{m.common_action_remove()}</Button>
			{/if}
		</div>
		{#if code}
			<p class="tg-code" role="status">
				{m.crew_settings_telegram_row_send_start_code()} <code>/start {code}</code>
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
