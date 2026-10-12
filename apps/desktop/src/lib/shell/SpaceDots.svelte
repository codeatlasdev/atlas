<script lang="ts">
	import { shell } from "#lib/shell/shell.svelte.ts";
	import { health, store } from "#lib/stores/projects.svelte.ts";
	import { identities } from "#lib/stores/identity.svelte.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import Tip from "#lib/components/Tip.svelte";

	/**
	 * Arc's space strip: one icon per project at the bottom of the sidebar (its emoji, icon, logo
	 * or monogram). The active one is ringed; a running stack breathes. Click to switch; ⌃1…⌃9
	 * and ⌥⌘←/→ do the same.
	 */
</script>

{#if shell.spaces.length > 1}
	<div class="spaces" role="tablist" aria-label="Projects" data-tour="spaces">
		{#each shell.spaces as s, i (s.id)}
			{@const on = s.id === shell.activeId}
			<Tip label={s.name} command={i < 9 ? `shell::Space${i + 1}` : undefined} side="top">
				{#snippet child(props)}
					<button
						{...props}
						class="space"
						class:on
						class:up={health(s, store.live[s.id]) === "ok"}
						role="tab"
						aria-selected={on}
						aria-label={s.name}
						onclick={() => shell.switchTo(s.id)}
						oncontextmenu={(e) => {
							e.preventDefault();
							identities.editing = s.id;
						}}
					>
						<ProjectIcon id={s.id} size={22} ring={on} />
					</button>
				{/snippet}
			</Tip>
		{/each}
	</div>
{/if}

<style>
	.spaces {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 4px;
		padding: 4px 4px 6px;
	}
	.space {
		display: grid;
		place-items: center;
		padding: 3px;
		border: 0;
		border-radius: 9px;
		background: none;
		opacity: 0.62;
		transition:
			opacity var(--dur) var(--ease),
			transform var(--dur) var(--ease);
	}
	.space:hover,
	.space.on {
		opacity: 1;
	}
	.space:hover:not(.on) {
		transform: translateY(-1px);
	}
	/* A running stack keeps its icon at full strength and breathes softly. */
	.space.up:not(.on) {
		opacity: 0.9;
		animation: breathe 3s ease-in-out infinite;
	}
	@keyframes breathe {
		50% {
			opacity: 0.68;
		}
	}
</style>
