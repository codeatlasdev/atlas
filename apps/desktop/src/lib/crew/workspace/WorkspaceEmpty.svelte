<script lang="ts">
	/** WorkspaceEmpty — the right panel when no crewmate is selected: a calm horizon and the recruit button. */
	import { recruit } from "#lib/crew/workspace/actions.svelte.ts";
</script>

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
			<line x1="14" y1="8" x2="14" y2="36" stroke="currentColor" stroke-width="1.5" opacity="0.38" />
			<path d="M14 10 L24 28 L14 28 Z" fill="currentColor" opacity="0.28" />
			<path d="M2 36 Q14 40 26 36 L24 34 L4 34 Z" fill="currentColor" opacity="0.32" />
		</g>
	</svg>
	<p class="empty-calm">No crew selected.</p>
	<p class="panel-empty-hint">Choose a crewmate or recruit your first one.</p>
	<button class="btn-primary-pill" onclick={() => void recruit.start()} disabled={recruit.creating}>
		Recruit a crewmate
	</button>
</div>

<style>
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
