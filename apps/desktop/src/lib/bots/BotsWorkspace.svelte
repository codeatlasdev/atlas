<script lang="ts">
	import { randomLook } from "#lib/bots/look.ts";
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
	import { Plus, Search, Settings, CircleHelp } from "@lucide/svelte";
	import { flip } from "svelte/animate";
	import BotTile from "#lib/bots/BotTile.svelte";
	import UsageMeter from "#lib/bots/UsageMeter.svelte";
	import BotOnboarding from "#lib/bots/BotOnboarding.svelte";

	// ─── state ────────────────────────────────────────────────────────────────

	let query = $state("");
	let creating = $state(false);
	let colWidth = $state(0);
	let searchOpen = $state(false);
	const tileMode = $derived(colWidth < 280 || bots.allBots.length <= 2);
	let selectedNav = $state<"bots" | "activity" | "triggers" | "needs_you">("bots");

	// ─── onboarding ───────────────────────────────────────────────────────────

	const seenOnboarding =
		typeof localStorage !== "undefined" && !!localStorage.getItem("bots:onboarding:seen");
	let showOnboarding = $state(!seenOnboarding && bots.allBots.length === 0);

	function closeOnboarding() {
		showOnboarding = false;
	}

	function openOnboarding() {
		showOnboarding = true;
	}

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

	type ProjectGroup = {
		projectName: string | null;
		bots: { hostKey: string; bot: (typeof bots.allBots)[number]["bot"] }[];
	};

	type Section = {
		hostKey: string;
		hostName: string;
		projects: ProjectGroup[];
	};

	const sections = $derived.by<Section[]>(() => {
		const map = new Map<string, Section>();
		for (const entry of filteredBots) {
			const host = bots.hosts.find((h) => h.key === entry.hostKey);
			const hostName = host?.name ?? "This computer";
			if (!map.has(entry.hostKey)) {
				map.set(entry.hostKey, { hostKey: entry.hostKey, hostName, projects: [] });
			}
			const section = map.get(entry.hostKey)!;
			const projectName = entry.bot.project?.name ?? null;
			let pg = section.projects.find((p) => p.projectName === projectName);
			if (!pg) {
				pg = { projectName, bots: [] };
				section.projects.push(pg);
			}
			pg.bots.push({ hostKey: entry.hostKey, bot: entry.bot });
		}
		return [...map.values()];
	});

	/** Per-agent usage summary for bottom bar meters. */
	const agentUsages = $derived.by(() => {
		const map = new Map<
			string,
			{
				agentId: string;
				todayTokens: number;
				todayCost: number;
				limitPct: number | null;
				resetsAt: string | null;
			}
		>();
		for (const { bot } of bots.allBots) {
			if (!bot.agent) continue;
			if (!map.has(bot.agent)) {
				map.set(bot.agent, {
					agentId: bot.agent,
					todayTokens: 0,
					todayCost: 0,
					limitPct: null,
					resetsAt: null,
				});
			}
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
		const pending = bots.pendingProjectLink;
		try {
			const hostKey = bots.hosts[0]?.key ?? "";
			const id = await bots.createBot(hostKey, {
				name: pending ? `${pending.name} Bot` : "New Bot",
				role: "",
				agent: "",
				approval: "auto",
				look: randomLook(),
			});
			if (pending) {
				await bots.updateBot(hostKey, id, { project: pending });
				bots.pendingProjectLink = null;
			}
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
	<div class="col-left" bind:clientWidth={colWidth}>
		<!-- Drag region -->
		<div class="col-drag" data-tauri-drag-region aria-hidden="true"></div>

		<!-- Top bar: search + add -->
		<div class="top-bar">
			{#if searchOpen}
				<input
					class="search"
					bind:value={query}
					placeholder="Find a bot…"
					aria-label="Search bots"
					onblur={() => {
						if (!query) searchOpen = false;
					}}
				/>
			{/if}
			<button class="icon-btn-round" onclick={() => (searchOpen = !searchOpen)} aria-label="Search">
				<Search size={14} />
			</button>
			<button class="icon-btn-round" onclick={startNewBot} aria-label="New bot" disabled={creating}>
				<Plus size={14} />
			</button>
		</div>

		<!-- Bot list -->
		<div class="bot-list" class:tile-mode={tileMode} role="list">
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
					{#each section.projects as pg (pg.projectName ?? "__none__")}
						{#if pg.projectName}
							<div class="project-hd">
								<span>{pg.projectName}</span>
							</div>
						{/if}
						{#each pg.bots as { hostKey, bot } (bot.id)}
							<div animate:flip={{ duration: 200 }}>
								{#if tileMode}
									<BotTile
										{bot}
										active={isActive(hostKey, bot.id)}
										unread={bots.hasUnread(hostKey, bot.id)}
										onclick={() => selectBot(hostKey, bot.id)}
									/>
								{:else}
									<BotListRow
										{bot}
										active={isActive(hostKey, bot.id)}
										unread={bots.hasUnread(hostKey, bot.id)}
										onclick={() => selectBot(hostKey, bot.id)}
										onContextAction={(action) => handleContextAction(hostKey, bot.id, action)}
									/>
								{/if}
							</div>
						{/each}
					{/each}
				{/each}
			{/if}
		</div>

		<!-- Server status footer (Discord user-bar style) -->
		<!-- Footer (Discord user-bar) -->
		<div class="dm-footer">
			<!-- Left: server status -->
			<div class="footer-servers">
				{#each remoteHosts as h (h.key)}
					<button
						class="footer-server"
						onclick={openBotsSettings}
						title={h.name}
						aria-label="{h.name} — {bots.hostOk(h.key) ? 'online' : 'offline'}"
					>
						<span class="server-dot" class:online={bots.hostOk(h.key)} aria-hidden="true"></span>
						<span class="server-name">{h.name}</span>
					</button>
				{/each}
			</div>
			<!-- Right: meters + help + gear -->
			<div class="footer-actions">
				{#if agentUsages.length > 0}
					<div class="footer-meters">
						{#each agentUsages as usage (usage.agentId)}
							<UsageMeter
								agentId={usage.agentId}
								todayTokens={usage.todayTokens}
								todayCost={usage.todayCost}
								limitPct={usage.limitPct}
								resetsAt={usage.resetsAt}
							/>
						{/each}
					</div>
				{/if}
				<button
					class="footer-icon-btn"
					onclick={openOnboarding}
					title="How Atlas Bot works"
					aria-label="How Atlas Bot works"
				>
					<CircleHelp size={14} />
				</button>
				<button
					class="footer-icon-btn"
					onclick={openBotsSettings}
					title="Settings"
					aria-label="Bots settings"
				>
					<Settings size={14} />
				</button>
			</div>
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

	{#if showOnboarding}
		<BotOnboarding ondone={closeOnboarding} />
	{/if}
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

	/* Top bar */
	.top-bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 4px;
		padding: 4px 8px;
		flex: none;
	}

	.icon-btn-round {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: none;
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: default;
		flex: none;
		transition: background 150ms;
		padding: 0;
	}
	.icon-btn-round:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.icon-btn-round:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.search {
		flex: 1;
		min-width: 0;
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

	/* Host header (multi-host only) */
	.host-hd {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
		padding: 10px 12px 2px;
	}

	/* Project sub-header within a host section */
	.project-hd {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.05em;
		color: var(--ca-text-secondary);
		padding: 8px 12px 2px;
		text-transform: uppercase;
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

	/* Footer (Discord user-bar style) */
	.dm-footer {
		flex: none;
		display: flex;
		flex-direction: row;
		align-items: center;
		justify-content: space-between;
		padding: 6px 8px;
		gap: 6px;
		min-height: 40px;
		border-top: 1px solid var(--ca-border, rgba(255, 255, 255, 0.06));
	}

	.footer-servers {
		display: flex;
		align-items: center;
		gap: 6px;
		flex: 1;
		min-width: 0;
		overflow: hidden;
	}

	.footer-server {
		display: flex;
		align-items: center;
		gap: 4px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--ca-text-secondary);
		font-size: 11px;
		padding: 2px 4px;
		border-radius: 4px;
		max-width: 120px;
		overflow: hidden;
	}

	.footer-server:hover {
		background: var(--ca-bg-hover, var(--ca-vibrancy-fill));
		color: var(--ca-text-primary);
	}

	.server-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		flex-shrink: 0;
	}

	.server-dot.online {
		background: #22c55e;
	}

	.server-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.footer-actions {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.footer-meters {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.footer-icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		border-radius: 6px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--ca-text-secondary);
		flex-shrink: 0;
	}

	.footer-icon-btn:hover {
		background: var(--ca-bg-hover, var(--ca-vibrancy-fill));
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

	/* Tile mode */
	.bot-list.tile-mode {
		display: flex;
		flex-direction: row;
		flex-wrap: wrap;
		justify-content: center;
		align-content: flex-start;
		gap: 4px;
		padding: 8px 4px;
	}
	.bot-list.tile-mode .host-hd,
	.bot-list.tile-mode .project-hd {
		width: 100%;
	}
</style>
