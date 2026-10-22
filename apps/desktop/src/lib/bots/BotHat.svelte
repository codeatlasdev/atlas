<script lang="ts">
	import { shade, type Hat } from "#lib/bots/look.ts";

	// Drawn in the body's coordinates (globe: centre 214,213, radius ~213; top at y≈0).
	let { hat, color }: { hat: Hat; color: string } = $props();
	const glow = $derived(shade(color, 0.55));
</script>

{#if hat === "cap"}
	<path d="M70 78 Q214 -96 358 78 Q214 46 70 78 Z" fill="#ef4444" />
	<path d="M300 64 Q412 44 452 80 Q376 98 296 84 Z" fill="#b91c1c" />
	<path d="M214 -8 L214 60" stroke="#b91c1c" stroke-width="6" opacity=".5" />
	<circle cx="214" cy="-10" r="12" fill="#b91c1c" />
{:else if hat === "beanie"}
	<path d="M66 92 Q214 -120 362 92 Z" fill="#f59e0b" />
	<path d="M58 96 Q214 40 370 96 L364 130 Q214 74 64 130 Z" fill="#d97706" />
	<g class="pom"><circle cx="214" cy="-22" r="30" fill="#fde68a" /></g>
{:else if hat === "tophat"}
	<g transform="rotate(-6 214 40)">
		<rect x="122" y="-132" width="184" height="170" rx="14" fill="#18181b" />
		<rect x="122" y="2" width="184" height="26" fill="#dc2626" />
		<ellipse cx="214" cy="40" rx="172" ry="26" fill="#27272a" />
	</g>
{:else if hat === "crown"}
	<path
		d="M112 56 L96 -58 L158 -4 L214 -86 L270 -4 L332 -58 L316 56 Q214 34 112 56 Z"
		fill="#facc15"
		stroke="#ca8a04"
		stroke-width="6"
		stroke-linejoin="round"
	/>
	<circle cx="214" cy="22" r="14" fill="#ef4444" />
	<circle cx="150" cy="30" r="10" fill="#22c55e" />
	<circle cx="278" cy="30" r="10" fill="#3b82f6" />
{:else if hat === "party"}
	<g transform="rotate(14 214 40)">
		<path d="M146 44 L214 -146 L282 44 Q214 28 146 44 Z" fill="#ec4899" />
		<path d="M170 -24 L258 -24 L268 4 L160 4 Z" fill="#fde047" opacity=".9" />
		<path d="M194 -86 L234 -86 L242 -64 L186 -64 Z" fill="#fde047" opacity=".9" />
		<g class="pom"><circle cx="214" cy="-150" r="18" fill="#a5f3fc" /></g>
	</g>
{:else if hat === "wizard"}
	<path d="M128 46 Q190 -40 228 -170 Q248 -60 300 46 Z" fill="#6d28d9" />
	<ellipse cx="214" cy="46" rx="170" ry="26" fill="#5b21b6" />
	<path
		class="sparkle"
		d="M214 -40 l9 20 21 3 -16 14 5 21 -19 -11 -19 11 5 -21 -16 -14 21 -3 Z"
		fill="#fde047"
	/>
{:else if hat === "headphones"}
	<path d="M30 210 Q24 -44 214 -44 Q404 -44 398 210" stroke="#27272a" stroke-width="28" fill="none" />
	<rect x="-14" y="148" width="64" height="124" rx="28" fill="#3f3f46" />
	<rect x="378" y="148" width="64" height="124" rx="28" fill="#3f3f46" />
	<rect x="4" y="170" width="16" height="80" rx="8" fill={glow} />
	<rect x="408" y="170" width="16" height="80" rx="8" fill={glow} />
{:else if hat === "antenna"}
	<path d="M214 14 Q210 -40 222 -92" stroke="#3f3f46" stroke-width="12" stroke-linecap="round" fill="none" />
	<circle class="beacon" cx="222" cy="-100" r="24" fill={glow} />
{/if}

<style>
	.pom {
		animation: pom 2.4s ease-in-out infinite;
		transform-origin: 214px 40px;
	}
	@keyframes pom {
		0%,
		100% {
			transform: rotate(-4deg);
		}
		50% {
			transform: rotate(4deg);
		}
	}
	.sparkle {
		animation: sparkle 2.2s ease-in-out infinite;
		transform-box: fill-box;
		transform-origin: center;
	}
	@keyframes sparkle {
		0%,
		100% {
			transform: scale(1) rotate(0deg);
			opacity: 1;
		}
		50% {
			transform: scale(0.8) rotate(20deg);
			opacity: 0.7;
		}
	}
	.beacon {
		animation: beacon 1.6s ease-in-out infinite;
	}
	@keyframes beacon {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.45;
		}
	}
</style>
