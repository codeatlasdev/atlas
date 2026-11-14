<script lang="ts">
	/** IntegrationDetail — one integration's page: header, description, accounts (GitHub only) and its skills. */
	import { ArrowLeft, Check, ExternalLink } from "@lucide/svelte";
	import BrandIcon from "#lib/crew/BrandIcon.svelte";
	import CloseButton from "#lib/crew/integrations/CloseButton.svelte";
	import GithubAccounts from "#lib/crew/integrations/GithubAccounts.svelte";
	import IntegrationCard from "#lib/crew/integrations/IntegrationCard.svelte";
	import type { Integration } from "#lib/crew/integrations.ts";

	let {
		integration,
		onback,
		onclose,
	}: { integration: Integration; onback: () => void; onclose: () => void } = $props();
</script>

<div class="detail-head">
	<button class="back-btn" onclick={onback} aria-label="Back to integrations">
		<ArrowLeft size={15} />
		<span>Integrations</span>
	</button>
	<CloseButton {onclose} />
</div>

<div class="detail-header">
	<div class="detail-icon-wrap" aria-hidden="true">
		<BrandIcon id={integration.id} size={48} />
	</div>
	<div class="detail-meta">
		<h2 class="detail-name">{integration.name}</h2>
		{#if integration.sourceUrl}
			<a
				class="detail-source"
				href={integration.sourceUrl}
				target="_blank"
				rel="noreferrer"
				aria-label="View source for {integration.name}"
			>
				View source
				<ExternalLink size={11} aria-hidden="true" />
			</a>
		{/if}
	</div>
	<div class="detail-actions"></div>
</div>

<p class="detail-description">{integration.description}</p>

{#if integration.id === "github"}
	<GithubAccounts />

	<IntegrationCard title="Skills" count={integration.skills.length}>
		<ul class="skills-list" aria-label="Integration skills">
			{#each integration.skills as skill (skill.label)}
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
	</IntegrationCard>
{/if}

<style>
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
</style>
