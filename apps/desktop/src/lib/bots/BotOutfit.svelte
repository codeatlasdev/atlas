<script lang="ts">
	import { shade, type Outfit } from "#lib/bots/look.ts";

	// Drawn inside the body group, under the face, clipped to the shell (clip id from BotFace).
	let { outfit, color, clip }: { outfit: Outfit; color: string; clip: string } = $props();

	const INK = "#14142b";
	const GOLD = "#e8c36a";
</script>

{#if outfit === "scarf"}
	<g clip-path="url(#{clip})">
		<path d="M0 304 Q214 350 428 304 L428 346 Q214 392 0 346 Z" fill="#e5484d" />
		<path
			d="M0 316 Q214 362 428 316"
			stroke="#ffffff"
			stroke-width="6"
			stroke-dasharray="18 22"
			fill="none"
			opacity=".7"
		/>
	</g>
	<path d="M300 340 l40 70 l-34 6 Z" fill="#c9373c" />
	<path d="M310 338 l-6 78 l-30 -10 Z" fill="#e5484d" />
	<circle cx="304" cy="338" r="16" fill="#c9373c" />
{:else if outfit === "coat"}
	<g clip-path="url(#{clip})">
		<path d="M0 312 Q214 300 428 312 L428 440 L0 440 Z" fill="#1c2b5a" />
		<path d="M150 300 L214 380 L278 300 Z" fill={shade(color, 0.15)} />
		<path d="M150 300 L196 390 L120 330 Z M278 300 L232 390 L308 330 Z" fill="#26386f" />
		<path d="M0 312 Q214 300 428 312" stroke={GOLD} stroke-width="7" fill="none" />
		{#each [340, 380] as y (y)}
			<circle cx="180" cy={y + 30} r="8" fill={GOLD} />
			<circle cx="248" cy={y + 30} r="8" fill={GOLD} />
		{/each}
	</g>
{:else if outfit === "apron"}
	<g clip-path="url(#{clip})">
		<path d="M120 300 L308 300 L330 440 L98 440 Z" fill="#c69a6d" />
		<rect x="170" y="340" width="88" height="46" rx="8" fill="#a97f55" />
		<path d="M188 340 v-14 M240 340 v-14" stroke="#6b7686" stroke-width="8" stroke-linecap="round" />
	</g>
	<path d="M120 300 L60 120 M308 300 L368 120" stroke="#a97f55" stroke-width="10" stroke-linecap="round" />
{:else if outfit === "satchel"}
	<path d="M70 80 L340 360" stroke="#7a4b2a" stroke-width="16" stroke-linecap="round" />
	<g transform="translate(330 340) rotate(-8)">
		<rect x="-10" y="0" width="110" height="84" rx="14" fill="#9b6236" />
		<path d="M-10 14 h110 v20 q-55 20 -110 0 Z" fill="#7a4b2a" />
		<circle cx="45" cy="40" r="7" fill={GOLD} />
	</g>
{:else if outfit === "tie"}
	<path d="M196 300 h36 l-6 18 h-24 Z" fill={INK} />
	<path d="M202 318 h24 l18 92 l-30 22 l-30 -22 Z" fill={INK} />
	<path d="M206 340 l30 20 M200 372 l40 26" stroke={shade(color, 0.4)} stroke-width="6" opacity=".6" />
{/if}
