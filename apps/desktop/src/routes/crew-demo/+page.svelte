<script lang="ts">
	import BotEditor from "#lib/bots/BotEditor.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import type { BotLook } from "#lib/bots/look.ts";

	const STATES: BotState[] = ["idle", "working", "needs_you", "offline"];

	// One crewmate per CodeAtlas product, dressed for its job.
	const CREW: { name: string; look: BotLook }[] = [
		{
			name: "Atlas",
			look: {
				color: "#3d5bff",
				eyes: "round",
				hat: "bicorne",
				tool: "compass",
				outfit: "coat",
				face: "none",
			},
		},
		{
			name: "Koatla",
			look: { color: "#22c55e", eyes: "happy", hat: "cap", tool: "wrench", outfit: "apron", face: "goggles" },
		},
		{
			name: "Zelvo",
			look: {
				color: "#f97316",
				eyes: "round",
				hat: "straw",
				tool: "pin",
				outfit: "satchel",
				face: "sunglasses",
			},
		},
		{
			name: "Analisa",
			look: {
				color: "#3d5bff",
				eyes: "round",
				hat: "none",
				tool: "magnifier",
				outfit: "tie",
				face: "round-glasses",
			},
		},
		{
			name: "atla.bio",
			look: {
				color: "#8b5cf6",
				eyes: "round",
				hat: "bandana",
				tool: "megaphone",
				outfit: "scarf",
				face: "none",
			},
		},
		{
			name: "Promptin",
			look: {
				color: "#eab308",
				eyes: "round",
				hat: "beanie",
				tool: "quill",
				outfit: "scarf",
				face: "monocle",
			},
		},
		{
			name: "Vigia",
			look: {
				color: "#14b8a6",
				eyes: "round",
				hat: "bandana",
				tool: "spyglass",
				outfit: "none",
				face: "eyepatch",
			},
		},
		{
			name: "Noite",
			look: {
				color: "#8b5cf6",
				eyes: "sleepy",
				hat: "wizard",
				tool: "lantern",
				outfit: "coat",
				face: "none",
			},
		},
	];

	let crewState = $state<BotState>("idle");
	let building = $state(false);
	let draft = $state<BotLook>({
		color: "#38bdf8",
		eyes: "round",
		hat: "none",
		tool: "none",
		outfit: "none",
		face: "none",
	});
</script>

<div class="page">
	<div class="states" role="radiogroup" aria-label="State">
		{#each STATES as s (s)}
			<button
				role="radio"
				aria-checked={crewState === s}
				class:on={crewState === s}
				onclick={() => (crewState = s)}>{s}</button
			>
		{/each}
		<button class="build" aria-pressed={building} onclick={() => (building = !building)}>
			{building ? "Back to the crew" : "Build a crewmate"}
		</button>
	</div>
	{#if building}
		<div class="editor"><BotEditor bind:look={draft} seed="builder" /></div>
	{:else}
		<div class="grid">
			{#each CREW as c (c.name)}
				<div class="cell">
					<BotFace seed={c.name} look={c.look} state={crewState} size={150} track />
					<span class="lbl">{c.name}</span>
				</div>
			{/each}
		</div>
	{/if}
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
	.states .build {
		margin-left: 16px;
		border-color: var(--ca-brand-main, #3d5bff);
	}
	.editor {
		width: min(960px, 100%);
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
