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
	import { Plus, Search, Settings, CircleHelp, User } from "@lucide/svelte";
	import { crewOnboarding, SEEN_KEY } from "#lib/bots/crewOnboarding.svelte.ts";
	import { flip } from "svelte/animate";
	import UsageMeter from "#lib/bots/UsageMeter.svelte";
	import BotOnboarding from "#lib/bots/BotOnboarding.svelte";

	// ─── state ────────────────────────────────────────────────────────────────

	let query = $state("");
	let creating = $state(false);
	let searchOpen = $state(false);
	let footerOpen = $state(false);

	// ─── onboarding ───────────────────────────────────────────────────────────

	// First run: no crew yet and never seen. Replays (the "?" button, the palette, Settings) set the same shared flag.
	if (typeof localStorage !== "undefined" && !localStorage.getItem(SEEN_KEY) && bots.allBots.length === 0)
		crewOnboarding.open = true;

	function closeOnboarding() {
		crewOnboarding.open = false;
	}

	function openOnboarding() {
		crewOnboarding.open = true;
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
		} catch (e) {
			toast.error(errText(e));
		} finally {
			creating = false;
		}
	}

	function handleContextAction(hostKey: string, botId: string, action: "profile" | "delete") {
		if (action === "delete") {
			if (confirm("Delete this bot?")) void bots.deleteBot(hostKey, botId);
		} else {
			void bots.select(hostKey, botId);
		}
	}

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
				<Search size={16} />
			</button>
			<button class="icon-btn-round" onclick={startNewBot} aria-label="New crewmate" disabled={creating}>
				<Plus size={16} />
			</button>
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
							size={56}
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
			<!-- empty — footer "Launch your ship" is the CTA -->
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

		<!-- Footer: user avatar + ship pill -->
		<div class="dm-footer">
			<button class="footer-user-btn" aria-label="User">
				<User size={15} />
			</button>
			<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="footer-pill-wrap" onmouseleave={() => (footerOpen = false)}>
			{#if remoteHosts.length === 0}
				<button
					class="footer-pill footer-pill--launch"
					onclick={openOnboarding}
					aria-label="Launch your ship"
				>
					<ShipFlag size={13} />
					<span>Launch your ship</span>
				</button>
			{:else}
				<button
					class="footer-pill"
					onclick={() => (footerOpen = !footerOpen)}
					aria-label={remoteHosts[0].name}
					aria-expanded={footerOpen}
					aria-haspopup="menu"
				>
					<ShipFlag size={13} />
					<span class="server-dot" class:online={bots.hostOk(remoteHosts[0].key)} aria-hidden="true"></span>
					<span class="footer-ship-name">{remoteHosts[0].name}</span>
				</button>
				{#if footerOpen}
					<div class="footer-popover" role="menu">
						<button
							class="popover-item"
							role="menuitem"
							onclick={() => { openBotsSettings(); footerOpen = false; }}
						>
							<Settings size={13} />
							<span>Ship settings</span>
						</button>
						<button
							class="popover-item"
							role="menuitem"
							onclick={() => { openOnboarding(); footerOpen = false; }}
						>
							<CircleHelp size={13} />
							<span>Help</span>
						</button>
					</div>
				{/if}
			{/if}
		</div>
	</div>
	</div>

	<!-- Right panel -->
	<div class="col-right">
		{#if bots.selectedBot}
			<BotView />
		{:else}
			<div class="panel-empty">
				<svg
					class="horizon-art"
					width="120"
					height="64"
					viewBox="0 0 120 64"
					fill="none"
					xmlns="http://www.w3.org/2000/svg"
					aria-hidden="true"
				>
					<path d="M0 46 Q30 42 60 46 Q90 50 120 46" stroke="currentColor" stroke-width="1" opacity="0.2" fill="none" />
					<path d="M0 52 Q30 48 60 52 Q90 56 120 52" stroke="currentColor" stroke-width="1" opacity="0.12" fill="none" />
					<line x1="0" y1="44" x2="120" y2="44" stroke="currentColor" stroke-width="1.5" opacity="0.28" />
					<g transform="translate(46, 8)">
						<line x1="14" y1="8" x2="14" y2="36" stroke="currentColor" stroke-width="1.5" opacity="0.38" />
						<path d="M14 10 L24 28 L14 28 Z" fill="currentColor" opacity="0.28" />
						<path d="M2 36 Q14 40 26 36 L24 34 L4 34 Z" fill="currentColor" opacity="0.32" />
					</g>
				</svg>
				<p class="empty-calm">No crew selected.</p>
				<p class="panel-empty-hint">Choose a crewmate or recruit your first one.</p>
				<button class="btn-primary-pill" onclick={startNewBot} disabled={creating}>Recruit a crewmate</button>
			</div>
		{/if}
	</div>

	{#if crewOnboarding.open}
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
		width: 270px;
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
		width: 36px;
		height: 36px;
		border-radius: 50%;
		border: none;
		background: var(--ca-surface-raised, var(--ca-vibrancy-fill));
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
		width: 56px;
		height: 56px;
	}
	.hero-dots {
		width: 56px;
		height: 56px;
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


	.empty-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}


	/* Footer (Discord user-bar style) */
	/* ─── Footer: user + ship pill ────────────────────────────── */
	.dm-footer {
		flex: none;
		display: flex;
		flex-direction: row;
		align-items: center;
		gap: 8px;
		padding: 8px;
		min-height: 48px;
		border-top: 1px solid var(--ca-border, rgba(255, 255, 255, 0.06));
	}


	.footer-user-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: none;
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-secondary);
		cursor: default;
		flex: none;
		padding: 0;
	}
	.footer-user-btn:hover {
		color: var(--ca-text-primary);
	}
	.footer-pill-wrap {
		flex: 1;
		min-width: 0;
		position: relative;
	}
	.footer-pill {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 6px 10px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		font-size: 12px;
		font-weight: 500;
		cursor: default;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		transition: background 120ms;
	}
	.footer-pill:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.footer-pill--launch {
		background: var(--ca-brand-main);
		color: var(--ca-surface-base);
		height: 40px;
		font-weight: 600;
		border-color: transparent;
	}
	.footer-pill--launch:hover {
		opacity: 0.88;
		color: var(--ca-surface-base);
		background: var(--ca-brand-main);
	}
	.footer-ship-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.footer-popover {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 0;
		right: 0;
		background: var(--ca-surface-overlay, var(--ca-surface-base));
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.22);
		padding: 4px;
		z-index: 100;
	}
	.popover-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		border: none;
		background: none;
		border-radius: 7px;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
		text-align: left;
	}
	.popover-item:hover {
		background: var(--ca-vibrancy-fill);
	}

	/* ─── Right panel ─────────────────────────────────────────── */
	.col-right {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	/* ─── Right panel empty state ──────────────────────────────── */
	.panel-empty {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 32px;
		text-align: center;
	}
	.panel-empty-hint {
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
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
</style>
