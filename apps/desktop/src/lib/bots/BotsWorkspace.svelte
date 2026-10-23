<script lang="ts">
	/**
	 * BotsWorkspace — full-window messenger layout (Discord DM style).
	 * Left column: drag region + search + nav + bot list + server status footer.
	 * Right panel: BotView chat for the selected bot, or activity/triggers/needs-you panels.
	 */
	import { randomLook, normalizeLook } from "#lib/bots/look.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import BotListRow from "#lib/bots/BotListRow.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import BotView from "../../routes/(app)/BotView.svelte";
	import { Plus, Search, Settings, CircleHelp, Compass, BellRing, Telescope } from "@lucide/svelte";
	import { flip } from "svelte/animate";
	import UsageMeter from "#lib/bots/UsageMeter.svelte";
	import BotOnboarding from "#lib/bots/BotOnboarding.svelte";

	// ─── state ────────────────────────────────────────────────────────────────

	let query = $state("");
	let creating = $state(false);
	let searchOpen = $state(false);
	let selectedNav = $state<"bots" | "activity" | "triggers" | "needs_you">("activity");

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

	// ─── favorite management ──────────────────────────────────────────────────

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
	function loadFav(): Map<string, string> {
		try {
			return new Map(Object.entries(JSON.parse(localStorage.getItem("bots:favorite") ?? "{}")));
		} catch {
			return new Map();
		}
	}

	function saveFav(m: Map<string, string>) {
		try {
			localStorage.setItem("bots:favorite", JSON.stringify(Object.fromEntries(m)));
		} catch {}
	}

	let favMap = $state(loadFav());

	const favoriteEntry = $derived.by(() => {
		for (const [hk, bid] of favMap) {
			const e = bots.allBots.find((x) => x.hostKey === hk && x.bot.id === bid);
			if (e) return e;
		}
		// default = first bot
		return bots.allBots[0] ?? null;
	});

	const nonFavBots = $derived(
		favoriteEntry
			? filteredBots.filter(
					(x) => !(x.hostKey === favoriteEntry!.hostKey && x.bot.id === favoriteEntry!.bot.id),
				)
			: filteredBots,
	);

	function setFavorite(hostKey: string, botId: string) {
		const m = new Map(favMap);
		m.delete(hostKey);
		m.set(hostKey, botId);
		favMap = m;
		saveFav(m);
	}

	const needsYouCount = $derived(bots.needsYouCount);

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
		selectedNav = "bots";
		void bots.select(hostKey, botId);
	}

	function navClick(nav: "activity" | "triggers" | "needs_you") {
		selectedNav = nav;
		bots.selected = null;
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
				name: pending ? `${pending.name} Crewmate` : "New Crewmate",
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
			selectedNav = "bots";
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

	function fmtActivityTime(tsMs: number): string {
		const diff = Date.now() - tsMs;
		const m = Math.floor(diff / 60000);
		if (m < 1) return "just now";
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return new Date(tsMs).toLocaleDateString([], { month: "short", day: "numeric" });
	}

	/** Map an hour (0–23) to its watch name. */
	function watchName(h: number): string {
		if (h >= 4 && h < 8) return "Morning watch · 04–08";
		if (h >= 8 && h < 12) return "Forenoon watch · 08–12";
		if (h >= 12 && h < 16) return "Afternoon watch · 12–16";
		if (h >= 16 && h < 20) return "Dog watches · 16–20";
		if (h >= 20 && h < 24) return "First watch · 20–24";
		return "Middle watch · 00–04";
	}

	/** Key for grouping activity entries: "YYYY-MM-DD·watch" */
	function watchKey(tsMs: number): string {
		const d = new Date(tsMs);
		const ymd = d.toISOString().slice(0, 10);
		return `${ymd}·${watchName(d.getHours())}`;
	}

	/** Ship-log time string "HH:MM" */
	function logTime(tsMs: number): string {
		return new Date(tsMs).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
	}

	/** Format a logbook row as ship-log voice. */
	function logEntry(botName: string, state: string, text: string): string {
		const snippet = text.slice(0, 60);
		if (state === "done") return `${botName} finished: ${snippet}`;
		if (state === "failed") return `${botName} failed: ${snippet}`;
		return `${botName}: ${snippet}`;
	}

	type WatchGroup = {
		key: string;
		label: string;
		date: string;
		entries: (typeof bots.activityFeed)[number][];
	};

	const watchGroups = $derived.by<WatchGroup[]>(() => {
		const map = new Map<string, WatchGroup>();
		for (const entry of bots.activityFeed) {
			const tsMs = entry.task.ts * 1000;
			const key = watchKey(tsMs);
			const d = new Date(tsMs);
			const isToday = d.toDateString() === new Date().toDateString();
			const dateLabel = isToday ? "Today" : d.toLocaleDateString([], { month: "short", day: "numeric" });
			if (!map.has(key)) {
				map.set(key, {
					key,
					label: watchName(d.getHours()),
					date: dateLabel,
					entries: [],
				});
			}
			map.get(key)!.entries.push(entry);
		}
		return [...map.values()];
	});
</script>

<div class="bots-workspace">
	<!-- Cartography grid overlay -->
	<svg class="crew-grid-bg" aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
		<defs>
			<pattern id="crew-grid" width="40" height="40" patternUnits="userSpaceOnUse">
				<path d="M 40 0 L 0 0 0 40" fill="none" stroke="rgba(255,255,255,0.07)" stroke-width="0.5" />
			</pattern>
		</defs>
		<rect width="100%" height="100%" fill="url(#crew-grid)" />
	</svg>
	<!-- Left column -->
	<div class="col-left">
		<!-- Drag region -->
		<div class="col-drag" data-tauri-drag-region aria-hidden="true"></div>

		<!-- Top bar: search + add -->
		<div class="top-bar">
			{#if searchOpen}
				<input
					class="search"
					bind:value={query}
					placeholder="Find a crewmate…"
					aria-label="Search bots"
					onblur={() => {
						if (!query) searchOpen = false;
					}}
				/>
			{/if}
			<button class="icon-btn-round" onclick={() => (searchOpen = !searchOpen)} aria-label="Search">
				<Search size={14} />
			</button>
		</div>

		<!-- Nav rows: Activity / Triggers / Needs you -->
		<nav class="nav-list" aria-label="Crew navigation">
			<button class="nav-row" class:active={selectedNav === "activity"} onclick={() => navClick("activity")}>
				<Compass size={15} aria-hidden="true" />
				<span>Logbook</span>
			</button>
			<button class="nav-row" class:active={selectedNav === "triggers"} onclick={() => navClick("triggers")}>
				<Telescope size={15} aria-hidden="true" />
				<span>Watches</span>
			</button>
			<button
				class="nav-row"
				class:active={selectedNav === "needs_you"}
				onclick={() => navClick("needs_you")}
			>
				<BellRing size={15} aria-hidden="true" />
				<span>Captain's call</span>
				{#if needsYouCount > 0}
					<span class="nav-pill" aria-label="{needsYouCount} bots need attention">{needsYouCount}</span>
				{/if}
			</button>
		</nav>

		<!-- BOTS section header -->
		<div class="section-hd">
			<span>MUGIWARAS</span>
			<button class="section-add" onclick={startNewBot} aria-label="Recruit" disabled={creating}
				><Plus size={12} /></button
			>
		</div>

		<!-- Favorite hero -->
		{#if favoriteEntry && !query.trim()}
			{@const hero = favoriteEntry}
			{@const heroLook = normalizeLook(hero.bot.look, hero.bot.id)}
			<div class="hero-wrap">
				<button
					class="hero-btn"
					class:active={isActive(hero.hostKey, hero.bot.id)}
					onclick={() => selectBot(hero.hostKey, hero.bot.id)}
					aria-label="{hero.bot.name} (first mate)"
					aria-pressed={isActive(hero.hostKey, hero.bot.id)}
				>
					<div class="hero-face-wrap">
						{#if hero.bot.state === "working"}
							<div class="hero-dots" style:background={heroLook.color}>
								<span class="dot"></span><span class="dot"></span><span class="dot"></span>
							</div>
						{:else}
							<BotFace
								seed={hero.bot.id}
								look={hero.bot.look}
								state={hero.bot.state === "needs_you" ? "needs_you" : "idle"}
								size={80}
							/>
						{/if}
						<span class="hero-star" aria-hidden="true">★</span>
						<!-- presence badge -->
						<span
							class="hero-presence"
							class:green={hero.bot.state === "working"}
							class:amber={hero.bot.state === "needs_you"}
							aria-hidden="true"
						></span>
						{#if bots.hasUnread(hero.hostKey, hero.bot.id)}
							<span class="hero-unread" aria-label="Unread"></span>
						{/if}
					</div>
					<span class="hero-name">{hero.bot.name}</span>
				</button>
			</div>
		{/if}

		<!-- Bot list (non-favorite) -->
		<div class="bot-list" role="list">
			{#if nonFavBots.length === 0 && bots.allBots.length === 0}
				<div class="empty-state">
					<p class="empty-title">No crew yet</p>
					<p class="empty-sub">Launch your ship to get started.</p>
					<button class="empty-add" onclick={openBotsSettings}>Launch your ship</button>
				</div>
			{:else if nonFavBots.length === 0 && query.trim()}
				<div class="empty-state">
					<p class="empty-sub">No crewmates match "{query}"</p>
				</div>
			{:else}
				{#each sections.filter( (s) => nonFavBots.some((nb) => nb.hostKey === s.hostKey) ) as section (section.hostKey)}
					{#if sections.length > 1}
						<div class="host-hd">{section.hostName}</div>
					{/if}
					{#each section.projects as pg (pg.projectName ?? "__none__")}
						{#if pg.projectName}
							<div class="project-hd"><span>{pg.projectName}</span></div>
						{/if}
						{#each pg.bots.filter( ({ hostKey, bot }) => nonFavBots.some((nb) => nb.hostKey === hostKey && nb.bot.id === bot.id) ) as { hostKey, bot } (bot.id)}
							<div animate:flip={{ duration: 200 }}>
								<BotListRow
									{bot}
									active={isActive(hostKey, bot.id)}
									unread={bots.hasUnread(hostKey, bot.id)}
									isFavorite={favoriteEntry?.hostKey === hostKey && favoriteEntry?.bot.id === bot.id}
									onclick={() => selectBot(hostKey, bot.id)}
									onFavorite={() => setFavorite(hostKey, bot.id)}
									onContextAction={(action) => handleContextAction(hostKey, bot.id, action)}
								/>
							</div>
						{/each}
					{/each}
				{/each}
			{/if}
		</div>

		<!-- Server status footer -->
		<div class="dm-footer">
			<!-- Left: ShipFlag + ship status -->
			<div class="footer-servers">
				{#if remoteHosts.length === 0}
					<button class="footer-no-ship" onclick={openBotsSettings} aria-label="No ship — launch one">
						<ShipFlag size={14} />
						<span>No ship yet · <span class="footer-launch">Launch</span></span>
					</button>
				{:else}
					{#each remoteHosts as h (h.key)}
						<button
							class="footer-server"
							onclick={openBotsSettings}
							title={h.name}
							aria-label="{h.name} — {bots.hostOk(h.key) ? 'online' : 'offline'}"
						>
							<ShipFlag size={14} />
							<span class="server-dot" class:online={bots.hostOk(h.key)} aria-hidden="true"></span>
							<span class="server-name">{h.name}</span>
						</button>
					{/each}
				{/if}
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
					title="How Atlas Crew works"
					aria-label="How Atlas Crew works"
				>
					<CircleHelp size={14} />
				</button>
				<button
					class="footer-icon-btn"
					onclick={openBotsSettings}
					title="Settings"
					aria-label="Crew settings"
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
		{:else if selectedNav === "needs_you"}
			<!-- Needs-you screen -->
			<div class="nav-view">
				<div class="nav-view-hd">Captain's call</div>
				{#if bots.allBots.filter((b) => b.bot.state === "needs_you").length === 0}
					<div class="nav-view-empty">No crewmates need your attention right now.</div>
				{:else}
					<div class="needs-list">
						{#each bots.allBots.filter((b) => b.bot.state === "needs_you") as { hostKey, bot } (bot.id)}
							<div class="needs-card">
								<BotFace seed={bot.id} look={bot.look} state="needs_you" size={36} />
								<div class="needs-body">
									<span class="needs-name">{bot.name}</span>
									{#if bot.pending_approval}
										<span class="needs-tool">{bot.pending_approval.tool_name}</span>
									{/if}
								</div>
								<div class="needs-btns">
									{#if bot.pending_approval}
										<button
											class="needs-deny"
											onclick={() =>
												void bots
													.approve(bot.pending_approval!.pane_key, bot.pending_approval!.req, false)
													.catch(() => {})}>Deny</button
										>
										<button
											class="needs-allow"
											onclick={() => {
												void bots
													.approve(bot.pending_approval!.pane_key, bot.pending_approval!.req, true)
													.catch(() => {});
												selectBot(hostKey, bot.id);
											}}>Allow</button
										>
									{/if}
									<button class="needs-open" onclick={() => selectBot(hostKey, bot.id)}>Open</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{:else if selectedNav === "triggers"}
			<!-- Triggers screen -->
			<div class="nav-view">
				<div class="nav-view-hd">Watches</div>
				{#if bots.allBots.filter((b) => (b.bot.triggers ?? []).length > 0).length === 0}
					<div class="nav-view-empty">
						<p>No watches configured.</p>
						<p class="nav-view-hint">Select a crewmate and open its profile to add watches.</p>
					</div>
				{:else}
					<div class="triggers-list">
						{#each bots.allBots.filter((b) => (b.bot.triggers ?? []).length > 0) as { hostKey, bot } (bot.id)}
							<div class="trigger-group">
								<div class="trigger-group-name">{bot.name}</div>
								{#each bot.triggers ?? [] as t (t.id)}
									<div class="trigger-row">
										<span class="trigger-kind">{t.kind.type}</span>
										<span class="trigger-enabled" class:on={t.enabled}>{t.enabled ? "on" : "off"}</span>
									</div>
								{/each}
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{:else}
			<!-- Activity view (default when no bot selected) -->
			<div class="nav-view activity-view">
				{#if bots.activityFeed.length === 0}
					<div class="nav-view-empty">
						<svg
							class="horizon-art"
							width="120"
							height="64"
							viewBox="0 0 120 64"
							fill="none"
							xmlns="http://www.w3.org/2000/svg"
							aria-hidden="true"
						>
							<path
								d="M0 46 Q30 42 60 46 Q90 50 120 46"
								stroke="currentColor"
								stroke-width="1"
								opacity="0.2"
								fill="none"
							/>
							<path
								d="M0 52 Q30 48 60 52 Q90 56 120 52"
								stroke="currentColor"
								stroke-width="1"
								opacity="0.12"
								fill="none"
							/>
							<line x1="0" y1="44" x2="120" y2="44" stroke="currentColor" stroke-width="1.5" opacity="0.28" />
							<g transform="translate(46, 8)">
								<line
									x1="14"
									y1="8"
									x2="14"
									y2="36"
									stroke="currentColor"
									stroke-width="1.5"
									opacity="0.38"
								/>
								<path d="M14 10 L24 28 L14 28 Z" fill="currentColor" opacity="0.28" />
								<path d="M2 36 Q14 40 26 36 L24 34 L4 34 Z" fill="currentColor" opacity="0.32" />
							</g>
						</svg>
						<p class="empty-calm">Calm seas.</p>
						<p class="nav-view-hint">Nothing logged yet.</p>
						<button class="btn-primary-pill" onclick={startNewBot} disabled={creating}
							>Recruit a crewmate</button
						>
					</div>
				{:else}
					<div class="activity-hd">Logbook</div>
					<div class="activity-feed">
						{#each watchGroups as wg (wg.key)}
							<div class="watch-hd">
								<span class="watch-label">{wg.label}</span>
								<span class="watch-date">{wg.date}</span>
							</div>
							{#each wg.entries as entry (entry.botId + entry.task.id)}
								<button class="activity-row" onclick={() => selectBot(entry.hostKey, entry.botId)}>
									<BotFace
										seed={entry.botId}
										look={bots.allBots.find((x) => x.bot.id === entry.botId)?.bot.look}
										state="idle"
										size={32}
									/>
									<div class="activity-body">
										{#if entry.task.source === "trigger"}
											<div class="route-line" aria-hidden="true"></div>
										{/if}
										<span class="activity-log-time">{logTime(entry.task.ts * 1000)}</span>
										<span class="activity-preview"
											>{logEntry(
												entry.botName,
												entry.task.state,
												entry.task.result ?? entry.task.prompt,
											)}</span
										>
									</div>
									<span class="activity-time">{fmtActivityTime(entry.task.ts * 1000)}</span>
								</button>
							{/each}
						{/each}
					</div>
					{#if bots.allBots.some((b) => b.bot.state === "needs_you")}
						<div class="activity-needs-hd">Captain's call</div>
						{#each bots.allBots.filter((b) => b.bot.state === "needs_you") as { hostKey, bot } (bot.id)}
							<div class="needs-card inline">
								<BotFace seed={bot.id} look={bot.look} state="needs_you" size={32} />
								<span class="needs-name">{bot.name}</span>
								{#if bot.pending_approval}
									<div class="needs-btns">
										<button
											class="needs-deny"
											onclick={() =>
												void bots
													.approve(bot.pending_approval!.pane_key, bot.pending_approval!.req, false)
													.catch(() => {})}>Deny</button
										>
										<button
											class="needs-allow"
											onclick={() => {
												void bots
													.approve(bot.pending_approval!.pane_key, bot.pending_approval!.req, true)
													.catch(() => {});
												selectBot(hostKey, bot.id);
											}}>Allow</button
										>
									</div>
								{:else}
									<button class="needs-open" onclick={() => selectBot(hostKey, bot.id)}>Open</button>
								{/if}
							</div>
						{/each}
					{/if}
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
		position: relative;
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
		height: 12px;
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

	/* Nav rows */
	.nav-list {
		flex: none;
		padding: 4px 6px;
	}
	.nav-row {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		background: transparent;
		border-radius: 6px;
		font-size: 13px;
		color: var(--ca-text-secondary);
		cursor: default;
		text-align: left;
		transition:
			background 80ms,
			color 80ms;
		margin-bottom: 1px;
	}
	.nav-row:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.nav-row.active {
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
		color: var(--ca-text-primary);
		font-weight: 500;
	}
	.nav-pill {
		margin-left: auto;
		min-width: 18px;
		height: 18px;
		border-radius: 9px;
		background: var(--ca-semantic-error, #e03131);
		color: var(--ca-surface-base);
		font-size: 10px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 0 4px;
	}

	/* Section header */
	.section-hd {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px 4px;
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
		text-transform: uppercase;
	}
	.section-add {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 4px;
		border: none;
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: default;
		padding: 0;
	}
	.section-add:hover {
		color: var(--ca-text-primary);
		background: var(--ca-vibrancy-fill);
	}
	.section-add:disabled {
		opacity: 0.4;
	}

	/* Hero */
	.hero-wrap {
		flex: none;
		display: flex;
		justify-content: center;
		padding: 12px 8px 8px;
	}
	.hero-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
		background: transparent;
		border: none;
		border-radius: 12px;
		padding: 8px;
		cursor: default;
		transition: background 150ms;
	}
	.hero-btn:hover {
		background: var(--ca-vibrancy-fill);
	}
	.hero-btn.active {
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
	}
	.hero-face-wrap {
		position: relative;
		width: 80px;
		height: 80px;
	}
	.hero-dots {
		width: 80px;
		height: 80px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
	}
	.hero-star {
		position: absolute;
		top: -2px;
		right: -2px;
		font-size: 13px;
		color: var(--ca-warning, var(--ca-semantic-warning, oklch(75% 0.18 70)));
		line-height: 1;
		text-shadow: 0 0 4px rgba(0, 0, 0, 0.4);
		pointer-events: none;
	}
	.hero-presence {
		position: absolute;
		bottom: 4px;
		right: 14px;
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--ca-text-disabled, #888);
		border: 2px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.hero-presence.green {
		background: var(--ca-semantic-success, #2f9e44);
	}
	.hero-presence.amber {
		background: var(--ca-semantic-warning, #e67700);
	}
	.hero-unread {
		position: absolute;
		top: 2px;
		left: 2px;
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--ca-brand-main);
		border: 2px solid var(--ca-surface-sunken);
		pointer-events: none;
	}
	.hero-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-primary);
		max-width: 96px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* hero dots bounce */
	.hero-dots .dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: rgba(255, 255, 255, 0.85);
		animation: dot-bounce 1.2s ease-in-out infinite;
	}
	.hero-dots .dot:nth-child(2) {
		animation-delay: 0.15s;
	}
	.hero-dots .dot:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot-bounce {
		0%,
		80%,
		100% {
			transform: scale(0.8);
			opacity: 0.5;
		}
		40% {
			transform: scale(1.1);
			opacity: 1;
		}
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

	.btn-primary-pill {
		margin-top: 8px;
		height: 48px;
		padding: 0 24px;
		border-radius: 999px;
		border: none;
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		font-size: 13px;
		font-weight: 500;
		cursor: default;
	}
	.btn-primary-pill:hover {
		opacity: 0.85;
	}
	.btn-primary-pill:disabled {
		opacity: 0.4;
	}

	/* nav-view panels */
	.nav-view {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		padding: 16px 24px;
	}
	.activity-view {
		max-width: 760px;
		width: 100%;
		margin: 0 auto;
		align-self: center;
	}
	.nav-view-hd,
	.activity-hd,
	.activity-needs-hd {
		font-size: 15px;
		font-weight: 600;
		color: var(--ca-text-primary);
		margin-bottom: 12px;
		padding-bottom: 8px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.activity-needs-hd {
		margin-top: 20px;
	}
	.nav-view-empty {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		color: var(--ca-text-secondary);
		font-size: 13px;
		gap: 6px;
		text-align: center;
	}
	.nav-view-hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.activity-feed {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.activity-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 10px;
		border: none;
		background: transparent;
		border-radius: 8px;
		cursor: default;
		text-align: left;
		width: 100%;
		transition: background 80ms;
	}
	.activity-row:hover {
		background: var(--ca-vibrancy-fill);
	}
	.activity-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.activity-preview {
		font-size: 12px;
		color: var(--ca-text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.activity-time {
		font-size: 11px;
		color: var(--ca-text-secondary);
		flex: none;
	}

	/* needs-you cards */
	.needs-list,
	.triggers-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.needs-card {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		background: var(--ca-surface-sunken);
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
	}
	.needs-card.inline {
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, var(--ca-surface-sunken));
		border-color: color-mix(in oklab, var(--ca-semantic-warning) 20%, transparent);
	}
	.needs-body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.needs-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.needs-tool {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.needs-btns {
		display: flex;
		gap: 6px;
		flex: none;
	}
	.needs-deny,
	.needs-allow,
	.needs-open {
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		font-size: 12px;
		cursor: default;
		background: none;
		color: var(--ca-text-primary);
	}
	.needs-deny:hover {
		background: var(--ca-vibrancy-fill);
	}
	.needs-allow {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		border-color: transparent;
	}
	.needs-allow:hover {
		opacity: 0.85;
	}
	.needs-open {
		color: var(--ca-text-secondary);
	}
	.needs-open:hover {
		color: var(--ca-text-primary);
		background: var(--ca-vibrancy-fill);
	}

	/* triggers */
	.trigger-group {
		margin-bottom: 12px;
	}
	.trigger-group-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-primary);
		margin-bottom: 4px;
	}
	.trigger-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.trigger-kind {
		text-transform: capitalize;
	}
	.trigger-enabled {
		font-size: 10px;
		padding: 1px 5px;
		border-radius: 8px;
		background: var(--ca-vibrancy-fill);
	}
	.trigger-enabled.on {
		background: color-mix(in oklab, var(--ca-semantic-success) 15%, transparent);
		color: var(--ca-semantic-success);
	}
	/* ─── Crew grid background ────────────────────────────────── */
	.crew-grid-bg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		z-index: 0;
	}
	@media (prefers-color-scheme: light) {
		.crew-grid-bg path {
			stroke: rgba(0, 0, 0, 0.07);
		}
	}

	/* ─── Route line (bot-chain trigger) ──────────────────────── */
	.route-line {
		width: 24px;
		height: 2px;
		border-top: 2px dashed currentColor;
		opacity: 0.45;
		flex: none;
		animation: route-march 1.2s linear infinite;
	}
	@keyframes route-march {
		from {
			background-position: 0 0;
		}
		to {
			background-position: 16px 0;
		}
	}

	/* ─── No-ship footer ──────────────────────────────────────── */
	.footer-no-ship {
		display: flex;
		align-items: center;
		gap: 5px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--ca-text-secondary);
		font-size: 11px;
		padding: 2px 4px;
		border-radius: 4px;
	}
	.footer-no-ship:hover {
		color: var(--ca-text-primary);
		background: var(--ca-vibrancy-fill);
	}
	.footer-launch {
		color: var(--ca-brand-main);
	}

	/* ─── Logbook watch section headers ──────────────────────── */
	.watch-hd {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 10px 4px;
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.05em;
		color: var(--ca-text-secondary);
		text-transform: uppercase;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		margin-bottom: 2px;
	}
	.watch-date {
		font-size: 10px;
		font-weight: 400;
		text-transform: none;
		letter-spacing: 0;
	}

	/* Ship-log timestamp inline in activity row */
	.activity-log-time {
		font-size: 10.5px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
		flex: none;
	}

	/* Horizon empty state art */
	.horizon-art {
		color: var(--ca-text-secondary);
		margin-bottom: 4px;
	}
	.empty-calm {
		font-size: 14px;
		font-weight: 500;
		color: var(--ca-text-primary);
		margin: 0;
	}

	/* Captain's call lantern glow when non-empty */
	.nav-row:has(.nav-pill) {
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, transparent);
	}
</style>
