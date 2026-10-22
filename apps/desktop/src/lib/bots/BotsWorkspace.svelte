<script lang="ts">
	/**
	 * BotsWorkspace — full-window messenger layout.
	 * Left column: search + bot list + server status footer.
	 * Right panel: BotView chat for the selected bot, or an empty state.
	 */
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import BotListRow from "#lib/bots/BotListRow.svelte";
	import BotNewDialog from "#lib/components/BotNewDialog.svelte";
	import BotView from "../../routes/(app)/BotView.svelte";

	// ─── state ────────────────────────────────────────────────────────────────

	let query = $state("");
	let showNewBot = $state(false);

	// ─── derived ──────────────────────────────────────────────────────────────

	const remoteHosts = $derived(bots.hosts.filter((h) => h.key !== ""));

	const filteredBots = $derived(
		query.trim()
			? bots.allBots.filter(
					({ bot }) =>
						bot.name.toLowerCase().includes(query.toLowerCase()) ||
						(bot.last?.text ?? "").toLowerCase().includes(query.toLowerCase()),
				)
			: bots.allBots,
	);

	const botsShortcut = $derived(shortcutFor("atlas::ToggleBotsWorkspace") ?? "⌘⇧B");

	// ─── actions ──────────────────────────────────────────────────────────────

	function selectBot(hostKey: string, botId: string) {
		void bots.select(hostKey, botId);
	}

	function isActive(hostKey: string, botId: string): boolean {
		return bots.selected?.hostKey === hostKey && bots.selected?.botId === botId;
	}

	function openBotsSettings() {
		updates.show("bots");
	}

	function onBotCreated(hostKey: string, botId: string) {
		void bots.select(hostKey, botId);
	}
</script>

<div class="bots-workspace">
	<!-- Left column -->
	<div class="col-left">
		<!-- Header: drag region + workspace switcher + new bot button -->
		<div class="col-header" data-tauri-drag-region>
			<div class="mode-switch" title="Switch workspace ({botsShortcut})">
				<button
					class="mode-btn"
					class:active={ws.appMode === "ide"}
					onclick={() => (ws.appMode = "ide")}
					aria-label="IDE workspace">IDE</button
				>
				<button
					class="mode-btn"
					class:active={ws.appMode === "bots"}
					onclick={() => (ws.appMode = "bots")}
					aria-label="Bots workspace">Bots</button
				>
			</div>
			<button class="new-bot-btn" onclick={() => (showNewBot = true)} aria-label="New bot" title="New bot"
				>+</button
			>
		</div>

		<!-- Search -->
		<div class="search-wrap">
			<input class="search" bind:value={query} placeholder="Search" aria-label="Search bots" />
		</div>

		<!-- Bot list -->
		<div class="bot-list" role="list">
			{#each filteredBots as { hostKey, bot } (bot.id)}
				<BotListRow {bot} active={isActive(hostKey, bot.id)} onclick={() => selectBot(hostKey, bot.id)} />
			{:else}
				{#if bots.allBots.length === 0}
					<div class="empty-state">
						<p class="empty-title">No bots yet</p>
						<p class="empty-sub">Add a bot server to get started.</p>
						<button class="empty-add" onclick={openBotsSettings}>Add bot server</button>
					</div>
				{:else}
					<div class="empty-state">
						<p class="empty-sub">No bots match "{query}"</p>
					</div>
				{/if}
			{/each}
		</div>

		<!-- Server status footer -->
		<div class="col-footer">
			{#each remoteHosts as h (h.key)}
				<button
					class="server-row"
					onclick={openBotsSettings}
					title="Settings → Bots"
					aria-label="{h.name} — {bots.hostOk(h.key) ? 'online' : 'offline'}"
				>
					<span class="server-dot" class:online={bots.hostOk(h.key)}>●</span>
					<span class="server-name">{h.name}</span>
				</button>
			{:else}
				<button class="add-server-btn" onclick={openBotsSettings}> + Add bot server </button>
			{/each}
		</div>
	</div>

	<!-- Right panel -->
	<div class="col-right">
		{#if bots.selectedBot}
			<BotView />
		{:else}
			<div class="no-selection" aria-live="polite">
				{#if bots.allBots.length > 0}
					Select a bot to start chatting.
				{:else}
					<p class="empty-title">No bots yet</p>
					<p class="empty-sub">Add a bot server in Settings → Bots,<br />then create your first bot.</p>
					<button class="btn-primary" onclick={openBotsSettings}>Settings → Bots</button>
				{/if}
			</div>
		{/if}
	</div>
</div>

{#if showNewBot}
	<BotNewDialog bind:open={showNewBot} onCreated={onBotCreated} />
{/if}

<style>
	.bots-workspace {
		display: flex;
		width: 100%;
		height: 100%;
		background: var(--ca-surface-base);
		overflow: hidden;
	}

	/* ─── Left column ─────────────────────────────────────────── */
	.col-left {
		width: 280px;
		flex: none;
		display: flex;
		flex-direction: column;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
	}

	.col-header {
		height: 52px;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 10px 0 12px;
		flex: none;
	}

	.mode-switch {
		display: flex;
		gap: 1px;
		background: color-mix(in oklab, var(--ca-outline-subtle) 60%, transparent);
		border-radius: 7px;
		padding: 2px;
	}

	.mode-btn {
		font-size: 11.5px;
		font-weight: 500;
		padding: 3px 11px;
		border-radius: 5px;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		transition:
			background 80ms,
			color 80ms,
			box-shadow 80ms;
		line-height: 1.4;
	}

	.mode-btn.active {
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
	}

	.mode-btn:not(.active):hover {
		color: var(--ca-text-primary);
	}

	.new-bot-btn {
		width: 24px;
		height: 24px;
		border-radius: 6px;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 18px;
		font-weight: 300;
		line-height: 1;
		cursor: default;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
	}

	.new-bot-btn:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	/* Search */
	.search-wrap {
		padding: 6px 10px 8px;
		flex: none;
	}

	.search {
		width: 100%;
		height: 28px;
		border-radius: 8px;
		border: none;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		color: var(--ca-text-primary);
		font-size: 12.5px;
		padding: 0 10px;
		outline: none;
		box-sizing: border-box;
	}

	.search:focus {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
	}

	.search::placeholder {
		color: var(--ca-text-secondary);
	}

	/* Bot list */
	.bot-list {
		flex: 1;
		overflow-y: auto;
	}

	/* Empty states */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 32px 16px;
		gap: 6px;
		text-align: center;
	}

	.empty-title {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		margin: 0;
	}

	.empty-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	.empty-add {
		margin-top: 8px;
		font-size: 12px;
		padding: 5px 12px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-brand-main);
		cursor: default;
	}

	.empty-add:hover {
		background: var(--ca-vibrancy-fill);
	}

	/* Server status footer */
	.col-footer {
		flex: none;
		padding: 6px 8px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}

	.server-row {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		padding: 3px 5px;
		border-radius: 4px;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		cursor: default;
		min-width: 0;
	}

	.server-row:hover {
		background: var(--ca-vibrancy-fill);
	}

	.server-dot {
		font-size: 7px;
		flex: none;
		color: var(--ca-text-secondary);
	}

	.server-dot.online {
		color: var(--ca-semantic-success);
	}

	.server-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.add-server-btn {
		font-size: 11px;
		color: var(--ca-brand-main);
		background: none;
		border: none;
		padding: 3px 5px;
		cursor: default;
		width: 100%;
		text-align: left;
		border-radius: 4px;
	}

	.add-server-btn:hover {
		background: var(--ca-vibrancy-fill);
	}

	/* ─── Right panel ─────────────────────────────────────────── */
	.col-right {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	.no-selection {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		color: var(--ca-text-secondary);
		font-size: 13px;
		gap: 8px;
		text-align: center;
		padding: 40px;
	}

	.btn-primary {
		margin-top: 8px;
		font-size: 13px;
		padding: 7px 16px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-brand-main);
		cursor: default;
	}

	.btn-primary:hover {
		background: var(--ca-vibrancy-fill);
	}
</style>
