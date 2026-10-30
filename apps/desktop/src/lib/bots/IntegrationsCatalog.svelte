<script lang="ts">
	/**
	 * IntegrationsCatalog — full-catalog modal modelled on the Grok Bot integrations screen.
	 *
	 * Two views:
	 *   catalog  — searchable grid of all integrations with category chips.
	 *   detail   — per-integration page (accounts card + skills card).
	 *
	 * Only GitHub is a real integration today; every other entry shows "Coming soon".
	 * The GitHub connection UI (device flow, PAT input, per-ship consent) lives entirely
	 * inside this component so the old footer-popover sub-view can be removed.
	 */
	import { Dialog, Button } from "@codeatlasdev/ui";
	import BrandIcon from "#lib/bots/BrandIcon.svelte";
	import { ArrowLeft, Check, ExternalLink, X, ChevronRight } from "@lucide/svelte";
	import { githubCrew } from "#lib/bots/githubCrew.svelte.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import {
		CATALOG,
		CATEGORIES,
		filterCatalog,
		connectedIntegrationIds,
		canConnect,
		type Integration,
		type CatalogCategory,
	} from "./integrations.ts";

	interface Props {
		open?: boolean;
		detail?: string | null;
	}

	let { open = $bindable(false), detail = $bindable<string | null>(null) }: Props = $props();

	let query = $state("");
	let activeCategory = $state<CatalogCategory>("All");

	const currentIntegration = $derived(detail ? (CATALOG.find((i) => i.id === detail) ?? null) : null);
	const connectedIds = $derived(connectedIntegrationIds(githubCrew.connected));
	const connectedCount = $derived(connectedIds.size);
	const filtered = $derived(filterCatalog(CATALOG, query, activeCategory));

	// In default (All, no query) mode: recommended section = GitHub first, rest by category.
	const isDefaultView = $derived(activeCategory === "All" && !query.trim());
	const recommended = $derived(isDefaultView ? filtered.filter((i) => i.id === "github") : []);
	const rest = $derived(isDefaultView ? filtered.filter((i) => i.id !== "github") : filtered);

	const byCategory = $derived.by(() => {
		const map = new Map<string, Integration[]>();
		for (const i of rest) {
			const existing = map.get(i.category) ?? [];
			existing.push(i);
			map.set(i.category, existing);
		}
		return [...map.entries()];
	});

	// Connected app icons for the chip (only GitHub for now)
	const connectedIntegrations = $derived(CATALOG.filter((i) => connectedIds.has(i.id)));

	function close() {
		open = false;
		detail = null;
		query = "";
		activeCategory = "All";
		// Stop all active ship login flows.
		for (const hostKey of githubCrew.flows.keys()) {
			githubCrew.stopPolling(hostKey);
		}
	}

	function openDetail(id: string) {
		if (!canConnect(CATALOG.find((i) => i.id === id)!)) return;
		detail = id;
		query = "";
		activeCategory = "All";
	}

	function backToCatalog() {
		detail = null;
		for (const hostKey of githubCrew.flows.keys()) {
			githubCrew.stopPolling(hostKey);
		}
	}
</script>

<Dialog
	{open}
	onOpenChange={(o) => {
		if (!o) close();
	}}
	title="Integrations"
	chromeless
	size="xl"
>
	<div class="catalog-shell">
		{#if !detail}
			<!-- ─── Catalog view ──────────────────────────────────────────── -->
			<div class="catalog-head">
				<h2 class="catalog-title">Integrations</h2>
				<button class="close-btn" onclick={close} aria-label="Close integrations">
					<X size={16} />
				</button>
			</div>

			{#if connectedCount > 0}
				<div class="connected-chip">
					{#each connectedIntegrations as integ (integ.id)}
						<span class="connected-chip-icon" aria-hidden="true">
							{@render integIcon(integ.id, 14)}
						</span>
					{/each}
					<span>{connectedCount} connected</span>
					<ChevronRight size={11} aria-hidden="true" />
				</div>
			{/if}

			<div class="search-row">
				<label class="search-label">
					<svg
						class="search-icon"
						viewBox="0 0 16 16"
						aria-hidden="true"
						fill="none"
						stroke="currentColor"
						stroke-width="1.5"
					>
						<circle cx="6.5" cy="6.5" r="4" />
						<path d="M11 11l2.5 2.5" stroke-linecap="round" />
					</svg>
					<input
						class="search-input"
						type="search"
						placeholder="Search apps and skills"
						bind:value={query}
						aria-label="Search integrations"
					/>
				</label>
			</div>

			<div class="category-chips" role="group" aria-label="Filter by category">
				{#each CATEGORIES as cat (cat)}
					<button
						class="category-chip"
						class:active={activeCategory === cat}
						onclick={() => (activeCategory = cat)}
						aria-pressed={activeCategory === cat}
					>
						{cat}
					</button>
				{/each}
			</div>

			<div class="catalog-body">
				{#if recommended.length > 0}
					<section class="catalog-section" aria-label="Recommended for you">
						<div class="section-head">
							<span class="section-label">Recommended for you</span>
						</div>
						<div class="items-grid">
							{#each recommended as integ (integ.id)}
								{@render integRow(integ)}
							{/each}
						</div>
					</section>
				{/if}

				{#if byCategory.length > 0}
					{#each byCategory as [category, items] (category)}
						<section class="catalog-section" aria-label={category}>
							<div class="section-head">
								<span class="section-label">{category}</span>
							</div>
							<div class="items-grid">
								{#each items as integ (integ.id)}
									{@render integRow(integ)}
								{/each}
							</div>
						</section>
					{/each}
				{/if}

				{#if filtered.length === 0}
					<p class="empty-state">No integrations match your search.</p>
				{/if}
			</div>
		{:else if currentIntegration}
			<!-- ─── Detail view ───────────────────────────────────────────── -->
			<div class="detail-head">
				<button class="back-btn" onclick={backToCatalog} aria-label="Back to integrations">
					<ArrowLeft size={15} />
					<span>Integrations</span>
				</button>
				<button class="close-btn" onclick={close} aria-label="Close integrations">
					<X size={16} />
				</button>
			</div>

			<div class="detail-header">
				<div class="detail-icon-wrap" aria-hidden="true">
					{@render integIcon(currentIntegration.id, 48)}
				</div>
				<div class="detail-meta">
					<h2 class="detail-name">{currentIntegration.name}</h2>
					{#if currentIntegration.sourceUrl}
						<a
							class="detail-source"
							href={currentIntegration.sourceUrl}
							target="_blank"
							rel="noreferrer"
							aria-label="View source for {currentIntegration.name}"
						>
							View source
							<ExternalLink size={11} aria-hidden="true" />
						</a>
					{/if}
				</div>
				<div class="detail-actions"></div>
			</div>

			<p class="detail-description">{currentIntegration.description}</p>

		{#if currentIntegration.id === "github"}
			<!-- Accounts card -->
			<div class="detail-card">
				<h3 class="card-title">GitHub accounts on your ships</h3>
				{#each bots.hosts.filter((h) => h.key.startsWith('ssh:')) as ship (ship.key)}
					{@const status = githubCrew.statuses.get(ship.key)}
					{@const flow = githubCrew.flows.get(ship.key)}
					{@const polling = githubCrew.isPolling(ship.key)}
					<div class="ship-gh-row">
						<span class="ship-gh-name">{ship.name.replace(/^atlas-bot@/, '')}</span>
						{#if flow}
							<!-- Login flow in progress -->
							<div class="gh-flow-block">
								<p class="gh-flow-label">Enter this code at <a href={flow.verification_uri} target="_blank" rel="noreferrer" class="text-link">{flow.verification_uri}</a></p>
								<div class="gh-code-row">
									<code class="gh-code">{flow.user_code}</code>
									<button
										class="gh-copy-btn"
										onclick={() => { void navigator.clipboard.writeText(flow.user_code); }}
										aria-label="Copy code"
									>Copy</button>
								</div>
								{#if polling}
									<p class="gh-flow-waiting" role="status" aria-live="polite">Waiting for authorization…</p>
								{/if}
								<Button variant="ghost" size="sm" onclick={() => githubCrew.stopPolling(ship.key)}>Cancel</Button>
							</div>
						{:else if status?.login}
							<!-- Connected -->
							<span class="ship-gh-login">Connected as @{status.login}</span>
							<Button variant="ghost" size="sm" onclick={() => void githubCrew.logout(ship.key)}>Disconnect</Button>
						{:else}
							<!-- Not connected -->
							<span class="ship-gh-status-off">Not connected</span>
							<Button
								variant="filled"
								size="sm"
								onclick={() => void githubCrew.startLogin(ship.key)}
							>Connect</Button>
						{/if}
					</div>
				{:else}
					<p class="no-ships">No ships added yet. Add a ship in the Bots tab to connect GitHub.</p>
				{/each}
			</div>

			<!-- Skills card -->
			<div class="detail-card">
				<h3 class="card-title">
					Skills
					<span class="card-count">{currentIntegration.skills.length}</span>
				</h3>
				<ul class="skills-list" aria-label="Integration skills">
					{#each currentIntegration.skills as skill (skill.label)}
						<li class="skill-row" class:skill-row--unavailable={!skill.available}>
							{#if skill.available}
								<span class="skill-check" aria-hidden="true"><Check size={13} /></span>
							{:else}
								<span class="skill-soon" aria-label="Coming soon">–</span>
							{/if}
							<span class="skill-label">{skill.label}</span>
							{#if !skill.available}
								<span class="skill-badge">Coming soon</span>
							{/if}
						</li>
					{/each}
				</ul>
			</div>
		{/if}
		{/if}
	</div>
</Dialog>

<!-- ─── Snippets ─────────────────────────────────────────────────────────────── -->

{#snippet integRow(integ: Integration)}
	{@const connected = connectedIds.has(integ.id)}
	{@const real = canConnect(integ)}
	<div class="integ-row">
		<div class="integ-row-icon" aria-hidden="true">
			{@render integIcon(integ.id, 36)}
		</div>
		<div class="integ-row-body">
			<span class="integ-row-name">{integ.name}</span>
		</div>
		{#if real}
			<button
				class="integ-pill"
				class:integ-pill--connected={connected}
				onclick={() => openDetail(integ.id)}
				aria-label="{connected ? 'Manage' : 'Connect'} {integ.name}"
			>
				{#if connected}
					<Check size={11} aria-hidden="true" />
					Connected
				{:else}
					Connect
					<ChevronRight size={11} aria-hidden="true" />
				{/if}
			</button>
		{:else}
			<span class="integ-pill integ-pill--soon" aria-label="Coming soon">Coming soon</span>
		{/if}
	</div>
{/snippet}

{#snippet integIcon(id: string, size: number)}
	<BrandIcon {id} {size} />
{/snippet}

<style>
	/* ─── Shell ──────────────────────────────────────────────────── */
	.catalog-shell {
		display: flex;
		flex-direction: column;
		width: 100%;
		min-height: 520px;
		max-height: min(680px, 88svh);
		overflow: hidden;
		background: var(--ca-surface-base);
		border-radius: 14px;
	}

	/* ─── Catalog head ───────────────────────────────────────────── */
	.catalog-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 20px 20px 0;
		flex: none;
	}

	.catalog-title {
		font-size: 17px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0;
	}

	.close-btn {
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
		transition: background 120ms;
	}
	.close-btn:hover {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-primary);
	}

	/* ─── Connected chip ─────────────────────────────────────────── */
	.connected-chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin: 10px 20px 0;
		padding: 3px 8px 3px 6px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-semantic-success) 8%, transparent);
		align-self: flex-start;
	}
	.connected-chip-icon {
		display: flex;
		align-items: center;
	}

	/* ─── Search ─────────────────────────────────────────────────── */
	.search-row {
		padding: 12px 20px 0;
		flex: none;
	}

	.search-label {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 34px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-outline-subtle) 40%, transparent);
		padding: 0 12px;
		cursor: text;
	}
	.search-label:focus-within {
		border-color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-outline-subtle) 20%, transparent);
	}

	.search-icon {
		width: 14px;
		height: 14px;
		color: var(--ca-text-disabled);
		flex: none;
	}

	.search-input {
		flex: 1;
		border: none;
		background: transparent;
		color: var(--ca-text-primary);
		font-size: 13px;
		outline: none;
		min-width: 0;
	}
	.search-input::placeholder {
		color: var(--ca-text-disabled);
	}
	/* Reset browser default search input styling */
	.search-input::-webkit-search-decoration,
	.search-input::-webkit-search-cancel-button {
		display: none;
	}

	/* ─── Category chips ─────────────────────────────────────────── */
	.category-chips {
		display: flex;
		gap: 6px;
		padding: 12px 20px;
		overflow-x: auto;
		scrollbar-width: none;
		flex: none;
	}
	.category-chips::-webkit-scrollbar {
		display: none;
	}

	.category-chip {
		flex: none;
		padding: 4px 12px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary);
		font-size: 12.5px;
		cursor: default;
		white-space: nowrap;
		transition:
			background 120ms,
			color 120ms;
	}
	.category-chip:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.category-chip.active {
		background: var(--ca-brand-main);
		color: var(--ca-text-on-brand);
		border-color: transparent;
	}

	/* ─── Catalog body ───────────────────────────────────────────── */
	.catalog-body {
		flex: 1;
		overflow-y: auto;
		padding: 0 20px 20px;
	}

	.catalog-section {
		margin-bottom: 20px;
	}

	.section-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 8px;
	}

	.section-label {
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--ca-text-disabled);
	}

	/* Two-column grid on wide modal */
	.items-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 2px;
	}

	/* ─── Integration row ────────────────────────────────────────── */
	.integ-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border-radius: 8px;
		transition: background 100ms;
	}
	.integ-row:hover {
		background: var(--ca-vibrancy-fill);
	}

	.integ-row-icon {
		width: 36px;
		height: 36px;
		border-radius: 8px;
		overflow: hidden;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.integ-row-body {
		flex: 1;
		min-width: 0;
	}

	.integ-row-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
	}

	/* ─── Integration pills ──────────────────────────────────────── */
	.integ-pill {
		flex: none;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		border-radius: 999px;
		font-size: 11.5px;
		font-weight: 500;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: default;
		white-space: nowrap;
	}
	button.integ-pill:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.integ-pill--connected {
		border-color: transparent;
		background: color-mix(in oklab, var(--ca-semantic-success) 12%, transparent);
		color: var(--ca-semantic-success);
	}
	button.integ-pill--connected:hover {
		background: color-mix(in oklab, var(--ca-semantic-success) 20%, transparent);
		color: var(--ca-semantic-success);
	}
	.integ-pill--soon {
		opacity: 0.55;
		cursor: default;
	}

	/* ─── Empty state ────────────────────────────────────────────── */
	.empty-state {
		color: var(--ca-text-secondary);
		font-size: 13px;
		text-align: center;
		padding: 40px 0;
		margin: 0;
	}

	/* ─── Detail head ────────────────────────────────────────────── */
	.detail-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px 0;
		flex: none;
	}

	.back-btn {
		display: flex;
		align-items: center;
		gap: 5px;
		border: none;
		background: none;
		color: var(--ca-brand-main);
		font-size: 13px;
		cursor: default;
		padding: 4px 6px;
		border-radius: 6px;
		transition: background 120ms;
	}
	.back-btn:hover {
		background: var(--ca-vibrancy-fill);
	}

	/* ─── Detail header ──────────────────────────────────────────── */
	.detail-header {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 16px 20px 0;
		flex: none;
	}

	.detail-icon-wrap {
		width: 48px;
		height: 48px;
		border-radius: 12px;
		overflow: hidden;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.detail-meta {
		flex: 1;
		min-width: 0;
	}

	.detail-name {
		font-size: 20px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0 0 2px;
	}

	.detail-source {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 12px;
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	.detail-source:hover {
		text-decoration: underline;
	}

	.detail-actions {
		flex: none;
	}

	.detail-description {
		padding: 10px 20px 0;
		margin: 0;
		font-size: 13.5px;
		color: var(--ca-text-secondary);
		line-height: 1.5;
	}

	/* ─── Cards ──────────────────────────────────────────────────── */
	.detail-card {
		margin: 14px 20px 0;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		overflow: hidden;
	}

	.card-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		margin: 0;
		padding: 10px 14px;
		display: flex;
		align-items: center;
		gap: 6px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-outline-subtle) 30%, transparent);
	}

	.card-count {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 18px;
		height: 18px;
		padding: 0 5px;
		border-radius: 999px;
		background: var(--ca-vibrancy-fill);
		font-size: 10.5px;
		font-weight: 700;
		color: var(--ca-text-secondary);
	}

	/* ─── Skills list ────────────────────────────────────────────── */
	.skills-list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}

	.skill-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 14px;
		font-size: 13px;
		color: var(--ca-text-primary);
	}

	.skill-row--unavailable {
		color: var(--ca-text-disabled);
	}

	.skill-check {
		color: var(--ca-semantic-success);
		flex: none;
	}

	.skill-soon {
		width: 13px;
		text-align: center;
		color: var(--ca-text-disabled);
		flex: none;
	}

	.skill-label {
		flex: 1;
	}

	.skill-badge {
		font-size: 10.5px;
		padding: 2px 7px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		color: var(--ca-text-disabled);
		white-space: nowrap;
	}

	/* ─── Per-ship GitHub rows ────────────────────────────────────── */
	.ship-gh-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 14px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	.ship-gh-row:last-child { border-bottom: none; }
	.ship-gh-name {
		flex: 1;
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ship-gh-login {
		font-size: 13px;
		color: var(--ca-text-secondary);
	}
	.ship-gh-status-off {
		font-size: 13px;
		color: var(--ca-text-disabled);
	}
	.gh-flow-block {
		width: 100%;
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 0;
	}
	.gh-flow-label {
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.gh-code-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.gh-code {
		font-size: 1.5rem;
		font-weight: 700;
		letter-spacing: 0.1em;
		color: var(--ca-text-primary);
		background: var(--ca-surface-raised);
		border-radius: 6px;
		padding: 6px 12px;
	}
	.gh-copy-btn {
		font-size: 13px;
		color: var(--ca-brand-main);
		background: none;
		border: none;
		cursor: default;
		padding: 4px 8px;
	}
	.gh-copy-btn:hover {
		text-decoration: underline;
	}
	.gh-flow-waiting {
		font-size: 11.5px;
		color: var(--ca-text-disabled);
		margin: 0;
	}
	.no-ships {
		font-size: 13px;
		color: var(--ca-text-secondary);
		padding: 12px 14px;
		margin: 0;
	}
</style>
