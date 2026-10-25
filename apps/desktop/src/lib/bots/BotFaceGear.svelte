<script lang="ts">
	import type { Face } from "#lib/bots/look.ts";

	// Eyewear, drawn in the body's coordinates above the eyes. `px`/`py` are the pupil
	// offsets: lenses and patch drift a fraction of the look so the gear feels worn, not pasted.
	let { face, px = 0, py = 0 }: { face: Face; px?: number; py?: number } = $props();

	const INK = "#14142b";
	const BRASS = "#d6a84a";
	const BRASS_DARK = "#9c7424";
	const dx = $derived(px * 0.25);
	const dy = $derived(py * 0.25);
</script>

{#if face === "eyepatch"}
	<g class="gear" style:transform="translate({dx}px, {dy}px)">
		<path d="M128 112 L336 232" stroke={INK} stroke-width="8" stroke-linecap="round" opacity=".9" />
		<ellipse cx="170" cy="198" rx="48" ry="44" fill={INK} />
		<ellipse cx="156" cy="180" rx="14" ry="8" fill="#fff" opacity=".18" />
	</g>
{:else if face === "monocle"}
	<g class="gear" style:transform="translate({dx}px, {dy}px)">
		<circle cx="258" cy="195" r="48" fill="#bfe6ff" opacity=".22" />
		<circle cx="258" cy="195" r="48" fill="none" stroke={BRASS} stroke-width="9" />
		<circle cx="258" cy="195" r="48" fill="none" stroke={BRASS_DARK} stroke-width="2" opacity=".6" />
		<path
			d="M236 168 Q250 156 270 158"
			stroke="#fff"
			stroke-width="6"
			stroke-linecap="round"
			fill="none"
			opacity=".7"
		/>
		<path d="M300 228 Q316 290 292 340" stroke={BRASS} stroke-width="4" stroke-linecap="round" fill="none" />
	</g>
{:else if face === "round-glasses"}
	<g class="gear" style:transform="translate({dx}px, {dy}px)">
		{#each [170, 258] as cx (cx)}
			<circle {cx} cy="195" r="46" fill="#bfe6ff" opacity=".2" />
			<circle {cx} cy="195" r="46" fill="none" stroke={INK} stroke-width="8" />
			<path
				d="M{cx - 24} 170 Q{cx - 10} 158 {cx + 6} 160"
				stroke="#fff"
				stroke-width="6"
				stroke-linecap="round"
				fill="none"
				opacity=".7"
			/>
		{/each}
		<path d="M200 190 Q214 180 228 190" stroke={INK} stroke-width="8" stroke-linecap="round" fill="none" />
		<path d="M124 190 L96 176 M304 190 L332 176" stroke={INK} stroke-width="7" stroke-linecap="round" />
	</g>
{:else if face === "sunglasses"}
	<g class="gear" style:transform="translate({dx}px, {dy}px)">
		<path d="M200 184 Q214 176 228 184" stroke={INK} stroke-width="9" stroke-linecap="round" fill="none" />
		<path d="M126 180 L96 168 M302 180 L332 168" stroke={INK} stroke-width="8" stroke-linecap="round" />
		<path d="M118 172 H206 Q206 242 162 246 Q118 242 118 172 Z" fill={INK} />
		<path d="M222 172 H310 Q310 242 266 246 Q222 242 222 172 Z" fill={INK} />
		<path d="M132 184 L170 184 L146 222 Z" fill="#fff" opacity=".16" />
		<path d="M236 184 L274 184 L250 222 Z" fill="#fff" opacity=".16" />
	</g>
{:else if face === "goggles"}
	<g class="gear" style:transform="translate({dx}px, {dy}px)">
		<path
			d="M-6 190 H118 M310 190 H434"
			stroke={INK}
			stroke-width="22"
			stroke-linecap="round"
			opacity=".85"
		/>
		{#each [170, 258] as cx (cx)}
			<circle {cx} cy="195" r="50" fill="#9be7ff" opacity=".22" />
			<circle {cx} cy="195" r="50" fill="none" stroke={BRASS} stroke-width="14" />
			<circle {cx} cy="195" r="57" fill="none" stroke={BRASS_DARK} stroke-width="3" opacity=".6" />
			<path
				d="M{cx - 26} 170 Q{cx - 8} 154 {cx + 12} 158"
				stroke="#fff"
				stroke-width="7"
				stroke-linecap="round"
				fill="none"
				opacity=".75"
			/>
		{/each}
		<rect x="206" y="184" width="16" height="22" rx="5" fill={BRASS_DARK} />
	</g>
{/if}

<style>
	.gear {
		transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
</style>
