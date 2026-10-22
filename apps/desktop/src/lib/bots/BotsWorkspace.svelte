<script lang="ts">
	/**
	 * BotsWorkspace — full-window messenger layout (Discord DM style).
	 * Left column: drag region + search + bot list + server status footer.
	 * Right panel: BotView chat for the selected bot, or an empty state.
	 */
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import BotListRow from "#lib/bots/BotListRow.svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import BotView from "../../routes/(app)/BotView.svelte";
	import { Settings } from "@lucide/svelte";
	import { flip } from "svelte/animate";

	// ─── state ────────────────────────────────────────────────────────────────

	let query = $state("");
	let creating = $state(false);
	let selectedNav = $state<"bots" | "activity" | "routines" | "needs_you">("bots");

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

	// ─── sections: group by host when there are multiple hosts ────────────────

	type Section = {
		hostKey: string;
		hostName: string;
		bots: { hostKey: string; bot: (typeof bots.allBots)[number]["bot"] }[];
	};

	const sections = $derived.by<Section[]>(() => {
		const map = new Map<string, Section>();
		for (const entry of filteredBots) {
			const host = bots.hosts.find((h) => h.key === entry.hostKey);
			const hostName = host?.name ?? "This computer";
			if (!map.has(entry.hostKey)) {
				map.set(entry.hostKey, { hostKey: entry.hostKey, hostName, bots: [] });
			}
			map.get(entry.hostKey)!.bots.push({ hostKey: entry.hostKey, bot: entry.bot });
		}
		return [...map.values()];
	});

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

	async function startNewBot() {
		if (creating) return;
		creating = true;
		try {
			const hostKey = bots.hosts[0]?.key ?? "";
			const id = await bots.createBot(hostKey, {
				name: "New Bot",
				role: "",
				agent: "",
				approval: "auto",
			});
			await bots.select(hostKey, id);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			creating = false;
		}
	}

	function handleContextAction(hostKey: string, botId: string, action: "profile" | "routine" | "delete") {
		if (action === "delete") {
			if (confirm("Delete this bot?")) void bots.deleteBot(hostKey, botId);
		} else {
			void bots.select(hostKey, botId);
		}
	}
</script>

<div class="bots-workspace">
	<!-- Left column -->
	<div class="col-left">
		<!-- Drag region -->
		<div class="col-drag" data-tauri-drag-region aria-hidden="true"></div>

		<!-- Search pill -->
		<div class="search-wrap">
			<input
				class="search"
				bind:value={query}
				placeholder="Find or start a conversation…"
				aria-label="Search bots"
			/>
		</div>

		<!-- Nav rows (Activity, Routines, Needs you) -->
		<div class="nav-rows">
			<button
				class="nav-row"
				class:active={selectedNav === "activity"}
				onclick={() => (selectedNav = "activity")}
			>
				<span class="nav-icon">◎</span>
				<span class="nav-label">Activity</span>
			</button>
			<button
				class="nav-row"
				class:active={selectedNav === "routines"}
				onclick={() => (selectedNav = "routines")}
			>
				<span class="nav-icon">⟳</span>
				<span class="nav-label">Routines</span>
			</button>
			<button
				class="nav-row"
				class:active={selectedNav === "needs_you"}
				onclick={() => (selectedNav = "needs_you")}
			>
				<span class="nav-icon">⚠</span>
				<span class="nav-label">Needs you</span>
				{#if bots.needsYouCount > 0}
					<span class="nav-badge">{bots.needsYouCount}</span>
				{/if}
			</button>
		</div>

		<!-- Section header -->
		<div class="section-hd">
			<span class="section-label">BOTS</span>
			<button
				class="section-add"
				onclick={startNewBot}
				aria-label="New bot"
				title="New bot"
				disabled={creating}>+</button
			>
		</div>

		<!-- Bot list -->
		<div class="bot-list" role="list">
			{#if sections.length === 0}
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
			{:else}
				{#each sections as section (section.hostKey)}
					{#if sections.length > 1}
						<div class="host-hd">{section.hostName}</div>
					{/if}
					{#each section.bots as { hostKey, bot } (bot.id)}
						<div animate:flip={{ duration: 200 }}>
							<BotListRow
								{bot}
								active={isActive(hostKey, bot.id)}
								unread={bots.hasUnread(hostKey, bot.id)}
								onclick={() => selectBot(hostKey, bot.id)}
								onContextAction={(action) => handleContextAction(hostKey, bot.id, action)}
							/>
						</div>
					{/each}
				{/each}
			{/if}
		</div>

		<!-- Server status footer (Discord user-bar style) -->
		<div class="dm-footer">
			<div class="footer-servers">
				{#each remoteHosts as h (h.key)}
					<button
						class="footer-server"
						onclick={openBotsSettings}
						title="Settings → Bots"
						aria-label="{h.name} — {bots.hostOk(h.key) ? 'online' : 'offline'}"
					>
						<span class="server-dot" class:online={bots.hostOk(h.key)} aria-hidden="true"></span>
						<span class="server-name">{h.name}</span>
					</button>
				{:else}
					<button class="footer-server muted" onclick={openBotsSettings}>+ Add bot server</button>
				{/each}
			</div>
			<button
				class="footer-gear"
				onclick={openBotsSettings}
				title="Settings → Bots"
				aria-label="Bots settings"
			>
				<Settings size={14} />
			</button>
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
		width: 240px;
		flex: none;
		display: flex;
		flex-direction: column;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
	}

	/* Drag region at top */
	.col-drag {
		height: 52px;
		flex: none;
	}

	/* Search */
	.search-wrap {
		padding: 0 10px 8px;
		flex: none;
	}

	.search {
		width: 100%;
		height: 28px;
		border-radius: 14px;
		border: none;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		color: var(--ca-text-primary);
		font-size: 12.5px;
		padding: 0 12px;
		outline: none;
		box-sizing: border-box;
	}

	.search:focus {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
	}

	.search::placeholder {
		color: var(--ca-text-secondary);
	}

	/* Section header */
	.section-hd {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px 4px 12px;
		flex: none;
	}

	.section-label {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
	}

	.section-add {
		width: 16px;
		height: 16px;
		border-radius: 4px;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 16px;
		font-weight: 300;
		line-height: 1;
		cursor: default;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		padding: 0;
	}

	.section-add:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	/* Host header (multi-host only) */
	.host-hd {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
		padding: 10px 12px 2px;
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

	/* Footer (Discord user bar style) */
	.dm-footer {
		flex: none;
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 8px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		min-height: 36px;
	}

	.footer-servers {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.footer-server {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		padding: 2px 4px;
		border-radius: 4px;
		width: 100%;
		text-align: left;
		background: none;
		border: none;
		cursor: default;
		min-width: 0;
	}

	.footer-server:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}

	.footer-server.muted {
		color: var(--ca-brand-main);
	}

	.server-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		flex: none;
	}

	.server-dot.online {
		background: var(--ca-semantic-success);
	}

	.server-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
		min-width: 0;
	}

	.footer-gear {
		width: 24px;
		height: 24px;
		border-radius: 6px;
		border: none;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		display: flex;
		align-items: center;
		justify-content: center;
		flex: none;
		padding: 0;
	}

	.footer-gear:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
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

	/* Nav rows */
	.nav-rows {
		padding: 0 4px 4px;
		flex: none;
	}
	.nav-row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 5px 8px;
		border: none;
		background: none;
		border-radius: 4px;
		font-size: 13px;
		color: var(--ca-text-secondary);
		cursor: default;
		text-align: left;
	}
	.nav-row:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.nav-row.active {
		background: color-mix(in oklab, var(--ca-brand-main) 15%, transparent);
		color: var(--ca-text-primary);
	}
	.nav-icon {
		font-size: 14px;
		flex: none;
	}
	.nav-label {
		flex: 1;
	}
	.nav-badge {
		background: var(--ca-semantic-error, #e03131);
		color: var(--ca-surface-base);
		font-size: 10px;
		font-weight: 700;
		border-radius: 8px;
		padding: 1px 5px;
		min-width: 16px;
		text-align: center;
	}
</style>
