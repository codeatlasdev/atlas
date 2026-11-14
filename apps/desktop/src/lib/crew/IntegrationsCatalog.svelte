<script lang="ts">
	/**
	 * IntegrationsCatalog — full-catalog modal modelled on the Grok Crewmate integrations screen.
	 *
	 * Two views:
	 *   catalog  — searchable grid of all integrations with category chips (this file).
	 *   detail   — per-integration page (integrations/IntegrationDetail.svelte).
	 *
	 * Only GitHub is a real integration today; every other entry shows "Coming soon".
	 */
	import { Dialog } from "@codeatlasdev/ui";
	import BrandIcon from "#lib/crew/BrandIcon.svelte";
	import CloseButton from "#lib/crew/integrations/CloseButton.svelte";
	import IntegrationDetail from "#lib/crew/integrations/IntegrationDetail.svelte";
	import { Check, ChevronRight } from "@lucide/svelte";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
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
		// Stop all active crew server login flows.
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
				<CloseButton onclose={close} />
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
			<IntegrationDetail integration={currentIntegration} onback={backToCatalog} onclose={close} />
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
</style>
