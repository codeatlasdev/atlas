<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import {
		BOT_COLORS,
		EYE_LABELS,
		EYE_STYLES,
		HAT_LABELS,
		HATS,
		randomLook,
		type BotLook,
	} from "#lib/bots/look.ts";

	let { look = $bindable(), seed }: { look: BotLook; seed: string } = $props();

	const STATES: { id: BotState; label: string }[] = [
		{ id: "idle", label: "Idle" },
		{ id: "working", label: "Working" },
		{ id: "needs_you", label: "Needs you" },
		{ id: "offline", label: "Offline" },
	];
	let preview = $state<BotState>("idle");
</script>

<div class="wrap">
	<div class="editor">
		<div class="stage">
			<BotFace {seed} {look} state={preview} size={200} track />
			<div class="states" role="radiogroup" aria-label="Preview state">
				{#each STATES as s (s.id)}
					<button
						role="radio"
						aria-checked={preview === s.id}
						class:on={preview === s.id}
						onclick={() => (preview = s.id)}
					>
						{s.label}
					</button>
				{/each}
			</div>
		</div>

		<div class="controls">
			<section>
				<h4>Color</h4>
				<div class="swatches" role="radiogroup" aria-label="Color">
					{#each BOT_COLORS as c (c)}
						<button
							role="radio"
							aria-checked={look.color === c}
							aria-label={c}
							class="swatch"
							class:on={look.color === c}
							style:background={c}
							onclick={() => (look = { ...look, color: c })}
						></button>
					{/each}
					<label
						class="swatch custom"
						class:on={!BOT_COLORS.includes(look.color as never)}
						title="Custom color"
					>
						<input
							type="color"
							aria-label="Custom color"
							value={look.color}
							oninput={(e) => (look = { ...look, color: e.currentTarget.value })}
						/>
					</label>
				</div>
			</section>

			<section>
				<h4>Eyes</h4>
				<div class="grid" role="radiogroup" aria-label="Eyes">
					{#each EYE_STYLES as e (e)}
						<button
							role="radio"
							aria-checked={look.eyes === e}
							class="tile"
							class:on={look.eyes === e}
							onclick={() => (look = { ...look, eyes: e })}
						>
							<BotFace {seed} look={{ ...look, eyes: e, hat: "none" }} state="idle" size={52} />
							<span>{EYE_LABELS[e]}</span>
						</button>
					{/each}
				</div>
			</section>

			<section>
				<h4>Hat</h4>
				<div class="grid" role="radiogroup" aria-label="Hat">
					{#each HATS as h (h)}
						<button
							role="radio"
							aria-checked={look.hat === h}
							class="tile"
							class:on={look.hat === h}
							onclick={() => (look = { ...look, hat: h })}
						>
							<BotFace {seed} look={{ ...look, hat: h }} state="idle" size={52} />
							<span>{HAT_LABELS[h]}</span>
						</button>
					{/each}
				</div>
			</section>

			<Button variant="glass" onclick={() => (look = randomLook())}>Surprise me</Button>
		</div>
	</div>
</div>

<style>
	.wrap {
		container-type: inline-size;
	}
	@container (max-width: 480px) {
		.editor {
			grid-template-columns: 1fr;
		}
	}
	.editor {
		display: grid;
		grid-template-columns: 240px 1fr;
		gap: 24px;
		color: var(--ca-text-primary);
	}
	.stage {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		padding: 28px 12px 16px;
		border-radius: 16px;
		background: var(--ca-surface-sunken);
		align-self: start;
	}
	.states {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 4px;
	}
	.states button {
		font: inherit;
		font-size: 11px;
		padding: 3px 8px;
		border-radius: 999px;
		border: 1px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
	}
	.states button.on {
		color: var(--ca-text-primary);
		border-color: var(--ca-brand-main);
	}
	.controls {
		display: flex;
		flex-direction: column;
		gap: 18px;
		align-items: flex-start;
	}
	section {
		width: 100%;
	}
	h4 {
		margin: 0 0 8px;
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.swatch {
		width: 26px;
		height: 26px;
		border-radius: 50%;
		border: 2px solid transparent;
		outline: 2px solid transparent;
		padding: 0;
		transition: transform 120ms;
	}
	.swatch:hover {
		transform: scale(1.12);
	}
	.swatch.on {
		outline-color: var(--ca-brand-main);
		border-color: var(--ca-surface-base);
	}
	.custom {
		position: relative;
		overflow: hidden;
		background: conic-gradient(red, yellow, lime, cyan, blue, magenta, red);
	}
	.custom input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(78px, 1fr));
		gap: 8px;
	}
	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 10px 4px 6px;
		border-radius: 12px;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 11px;
		transition:
			transform 120ms,
			border-color 120ms;
	}
	.tile:hover {
		transform: translateY(-2px);
	}
	.tile.on {
		border-color: var(--ca-brand-main);
		color: var(--ca-text-primary);
	}
</style>
