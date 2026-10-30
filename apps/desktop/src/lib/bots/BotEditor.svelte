<script lang="ts">
	import { Dialog, Button } from "@codeatlasdev/ui";
	import { Shuffle } from "@lucide/svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import {
		BOT_COLORS,
		EYE_LABELS,
		EYE_STYLES,
		FACE_LABELS,
		FACES,
		HAT_LABELS,
		HATS,
		OUTFIT_LABELS,
		OUTFITS,
		randomLook,
		ROLE_KITS,
		TOOL_LABELS,
		TOOLS,
		type BotLook,
	} from "#lib/bots/look.ts";
	import type { EquipPart } from "#lib/bots/rig.ts";

	interface Props {
		open?: boolean;
		look: BotLook;
		seed: string;
		onsave?: (look: BotLook) => void;
	}

	let { open = $bindable(false), look, seed, onsave }: Props = $props();

	// Internal draft — reset every time the modal opens.
	let draft = $state<BotLook>({ color: "#000000", eyes: "round", hat: "none", tool: "none", outfit: "none", face: "none" });
	let prevOpen = false;
	$effect(() => {
		if (open && !prevOpen) draft = { ...look };
		prevOpen = open;
	});

	const STATES: { id: BotState; label: string }[] = [
		{ id: "idle", label: "Idle" },
		{ id: "working", label: "Working" },
		{ id: "needs_you", label: "Needs you" },
		{ id: "offline", label: "Offline" },
	];
	let preview = $state<BotState>("idle");

	// Every change to the look pings the preview: the crewmate hops and the new piece animates in.
	let react = $state<{ n: number; part: EquipPart }>({ n: 0, part: "kit" });
	function equip(part: EquipPart, next: Partial<BotLook>) {
		draft = { ...draft, ...next };
		react = { n: react.n + 1, part };
	}

	const changed = $derived(
		draft.color !== look.color ||
		draft.eyes !== look.eyes ||
		draft.hat !== look.hat ||
		draft.tool !== look.tool ||
		draft.outfit !== look.outfit ||
		draft.face !== look.face,
	);

	function handleSave() {
		onsave?.(draft);
		open = false;
	}
</script>

<Dialog bind:open title="Customize look" size="xl">
	<!-- The kit Dialog leaves the page only after every animation inside it ends (presence waits on
	     getAnimations({subtree:true})). The faces breathe and blink forever, so the closed dialog never left and its
	     invisible backdrop swallowed every click. Closing turns those animations off so the exit can finish. -->
	<div class="editor" class:closing={!open}>
		<!-- ── Left: live preview ──────────────────────────────────────────── -->
		<div class="stage">
			<div class="face-wrap">
				<BotFace {seed} look={draft} state={preview} size={180} track {react} />
			</div>
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
			<Button variant="ghost" size="sm" onclick={() => { draft = randomLook(); react = { n: react.n + 1, part: "kit" }; }}>
				<Shuffle size={13} />
				Randomize
			</Button>
		</div>

		<!-- ── Right: options ─────────────────────────────────────────────── -->
		<div class="options">
			<section>
				<h4>Role kits</h4>
				<div class="kits">
					{#each ROLE_KITS as k (k.id)}
						<button class="kit" onclick={() => equip("kit", k.kit)}>{k.label}</button>
					{/each}
				</div>
			</section>

			<section>
				<h4>Color</h4>
				<div class="swatches" role="radiogroup" aria-label="Color">
					{#each BOT_COLORS as c (c)}
						<button
							role="radio"
							aria-checked={draft.color === c}
							aria-label={c}
							class="swatch"
							class:on={draft.color === c}
							style:background={c}
							onclick={() => equip("color", { color: c })}
						></button>
					{/each}
					<label
						class="swatch custom"
						class:on={!BOT_COLORS.includes(draft.color as never)}
						title="Custom color"
					>
						<input
							type="color"
							aria-label="Custom color"
							value={draft.color}
							oninput={(e) => equip("color", { color: e.currentTarget.value })}
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
							aria-checked={draft.eyes === e}
							class="tile"
							class:on={draft.eyes === e}
							onclick={() => equip("eyes", { eyes: e })}
						>
							<BotFace {seed} look={{ ...draft, eyes: e, hat: "none", face: "none" }} state="idle" size={52} />
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
							aria-checked={draft.hat === h}
							class="tile"
							class:on={draft.hat === h}
							onclick={() => equip("hat", { hat: h })}
						>
							<BotFace {seed} look={{ ...draft, hat: h }} state="idle" size={52} />
							<span>{HAT_LABELS[h]}</span>
						</button>
					{/each}
				</div>
			</section>

			<section>
				<h4>Face</h4>
				<div class="grid" role="radiogroup" aria-label="Face">
					{#each FACES as f (f)}
						<button
							role="radio"
							aria-checked={draft.face === f}
							class="tile"
							class:on={draft.face === f}
							onclick={() => equip("face", { face: f })}
						>
							<BotFace {seed} look={{ ...draft, face: f }} state="idle" size={52} />
							<span>{FACE_LABELS[f]}</span>
						</button>
					{/each}
				</div>
			</section>

			<section>
				<h4>Tool</h4>
				<div class="grid" role="radiogroup" aria-label="Tool">
					{#each TOOLS as x (x)}
						<button
							role="radio"
							aria-checked={draft.tool === x}
							class="tile"
							class:on={draft.tool === x}
							onclick={() => equip("tool", { tool: x })}
						>
							<BotFace {seed} look={{ ...draft, tool: x }} state="idle" size={52} />
							<span>{TOOL_LABELS[x]}</span>
						</button>
					{/each}
				</div>
			</section>

			<section>
				<h4>Outfit</h4>
				<div class="grid" role="radiogroup" aria-label="Outfit">
					{#each OUTFITS as x (x)}
						<button
							role="radio"
							aria-checked={draft.outfit === x}
							class="tile"
							class:on={draft.outfit === x}
							onclick={() => equip("outfit", { outfit: x })}
						>
							<BotFace {seed} look={{ ...draft, outfit: x }} state="idle" size={52} />
							<span>{OUTFIT_LABELS[x]}</span>
						</button>
					{/each}
				</div>
			</section>
		</div>
	</div>

	{#snippet actions()}
		<Button variant="ghost" onclick={() => (open = false)}>Cancel</Button>
		<Button disabled={!changed} onclick={handleSave}>Save</Button>
	{/snippet}
</Dialog>

<style>
	.editor.closing :global(*) {
		animation: none !important;
		transition: none !important;
	}
	/* Inset from the dialog's edges: the kit Dialog body has no side padding at this size, and the preview and the
	   last tile column sat flush against the frame. */
	.editor {
		display: grid;
		grid-template-columns: 220px 1fr;
		gap: 28px;
		align-items: start;
		padding: 4px 24px 20px;
	}

	/* ── Stage (left) ──────────────────────────────────────────────────────── */
	.stage {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		padding: 28px 16px 20px;
		border-radius: 14px;
		background: var(--ca-surface-sunken);
	}

	.face-wrap {
		/* Stable bounding box so the column doesn't jump between states */
		width: 180px;
		height: 180px;
		display: flex;
		align-items: center;
		justify-content: center;
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
		padding: 3px 9px;
		border-radius: 999px;
		border: 1px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
		transition: border-color 100ms;
	}

	.states button.on {
		color: var(--ca-text-primary);
		border-color: var(--ca-brand-main);
	}

	/* ── Options (right) ───────────────────────────────────────────────────── */
	.options {
		display: flex;
		flex-direction: column;
		gap: 20px;
		min-width: 0;
	}

	section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	h4 {
		margin: 0;
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}

	/* Role kits */
	.kits {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.kit {
		font: inherit;
		font-size: 12px;
		padding: 5px 12px;
		border-radius: 999px;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		cursor: pointer;
		transition:
			transform 120ms,
			border-color 120ms;
	}
	.kit:hover {
		transform: translateY(-1px);
		border-color: var(--ca-brand-main);
	}
	.kit:active {
		transform: scale(0.96);
	}

	/* Color swatches */
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
		cursor: pointer;
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
		cursor: pointer;
	}

	.custom input {
		position: absolute;
		inset: 0;
		opacity: 0;
		cursor: pointer;
	}

	/* Eyes / Hat / Face / Tool / Outfit tile grids */
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
		gap: 8px;
		/* Each grid scrolls independently if it overflows; the modal never scrolls. The inner padding keeps a
		   selected tile's outline and its hover lift from being clipped by the scroll box. */
		overflow-y: auto;
		max-height: 200px;
		padding: 4px;
		margin: -4px;
	}

	.tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 8px 4px 6px;
		border-radius: 10px;
		border: 1.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 11px;
		cursor: pointer;
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
