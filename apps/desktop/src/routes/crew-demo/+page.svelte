<script lang="ts">
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import type { BotLook } from "#lib/bots/look.ts";

	const STATES: BotState[] = ["idle", "working", "needs_you", "offline"];

	// One crewmate per CodeAtlas product, dressed for its job.
	const CREW: { name: string; look: BotLook }[] = [
		{
			name: "Atlas",
			look: { color: "#3d5bff", eyes: "round", hat: "bicorne", tool: "compass", outfit: "coat" },
		},
		{
			name: "Koatla",
			look: { color: "#22c55e", eyes: "happy", hat: "cap", tool: "wrench", outfit: "apron" },
		},
		{
			name: "Zelvo",
			look: { color: "#f97316", eyes: "round", hat: "straw", tool: "pin", outfit: "satchel" },
		},
		{
			name: "Analisa",
			look: { color: "#3d5bff", eyes: "visor", hat: "none", tool: "magnifier", outfit: "tie" },
		},
		{
			name: "atla.bio",
			look: { color: "#8b5cf6", eyes: "round", hat: "bandana", tool: "megaphone", outfit: "scarf" },
		},
		{
			name: "Promptin",
			look: { color: "#eab308", eyes: "happy", hat: "beanie", tool: "quill", outfit: "scarf" },
		},
		{
			name: "Vigia",
			look: { color: "#14b8a6", eyes: "cyclops", hat: "bandana", tool: "spyglass", outfit: "none" },
		},
		{
			name: "Noite",
			look: { color: "#8b5cf6", eyes: "sleepy", hat: "wizard", tool: "lantern", outfit: "coat" },
		},
	];

	let state = $state<BotState>("idle");
</script>

<div class="page">
	<div class="states" role="radiogroup" aria-label="State">
		{#each STATES as s (s)}
			<button role="radio" aria-checked={state === s} class:on={state === s} onclick={() => (state = s)}
				>{s}</button
			>
		{/each}
	</div>
	<div class="grid">
		{#each CREW as c (c.name)}
			<div class="cell">
				<BotFace seed={c.name} look={c.look} {state} size={150} track />
				<span class="lbl">{c.name}</span>
			</div>
		{/each}
	</div>
</div>

<style>
	:global(body) {
		margin: 0;
		background: #f3efe6;
		font-family: system-ui;
	}
	.page {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 32px;
		padding: 40px;
		min-height: 100%;
		background: var(--ca-surface-base);
	}
	.states {
		display: flex;
		gap: 8px;
	}
	.states button {
		padding: 6px 14px;
		border-radius: 999px;
		border: 1px solid var(--ca-border-subtle, #ccc);
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.states .on {
		background: var(--ca-text-primary, #111);
		color: var(--ca-surface-base, #fff);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(4, 200px);
		gap: 40px 24px;
	}
	.cell {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
	}
	.lbl {
		font-size: 12px;
		color: var(--ca-text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
</style>
