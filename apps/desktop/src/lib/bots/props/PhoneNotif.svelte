<script lang="ts">
	let { size = 80 }: { size?: number } = $props();

	const w = $derived(size);
	const h = $derived((size * 110) / 64);
</script>

<!-- Phone with a detailed notification card, pill buttons, and tap ring animation -->
<svg viewBox="0 0 64 110" width={w} height={h} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Phone body -->
	<rect x="8" y="4" width="48" height="90" rx="10" fill="currentColor" opacity=".85" />
	<!-- Screen -->
	<rect x="12" y="12" width="40" height="74" rx="6" fill="currentColor" opacity=".45" />
	<!-- Notch -->
	<rect x="26" y="10" width="12" height="4" rx="2" fill="currentColor" opacity=".6" />
	<!-- Status bar dots -->
	<rect x="16" y="15" width="10" height="2" rx="1" fill="currentColor" opacity=".2" />
	<rect x="38" y="15" width="8" height="2" rx="1" fill="currentColor" opacity=".2" />
	<!-- Home indicator -->
	<rect x="24" y="82" width="16" height="3" rx="1.5" fill="currentColor" opacity=".35" />

	<!-- Notification card — slides up from bottom of screen area -->
	<g class="notif-card">
		<!-- Card background -->
		<rect x="14" y="26" width="36" height="48" rx="6" fill="var(--ca-surface-raised)" opacity=".97" />
		<!-- Card border -->
		<rect
			x="14"
			y="26"
			width="36"
			height="48"
			rx="6"
			stroke="currentColor"
			stroke-width=".75"
			fill="none"
			opacity=".12"
		/>

		<!-- Header row: bot face circle with two dot eyes + title squiggles -->
		<circle cx="21" cy="37" r="5" fill="currentColor" opacity=".65" />
		<!-- Eye dots (lighter — show as face cutout) -->
		<circle cx="19.5" cy="36.2" r=".9" fill="var(--ca-surface-raised)" opacity=".9" />
		<circle cx="22.5" cy="36.2" r=".9" fill="var(--ca-surface-raised)" opacity=".9" />

		<!-- Title squiggle -->
		<rect x="28.5" y="33" width="16" height="2.5" rx="1.25" fill="currentColor" opacity=".5" />
		<!-- Subtitle squiggle -->
		<rect x="28.5" y="38" width="10" height="2" rx="1" fill="currentColor" opacity=".3" />

		<!-- Divider -->
		<rect x="16" y="44" width="32" height=".75" rx=".375" fill="currentColor" opacity=".08" />

		<!-- Body line 1 -->
		<rect x="16" y="47" width="31" height="2" rx="1" fill="currentColor" opacity=".28" />
		<!-- Body line 2 -->
		<rect x="16" y="51" width="19" height="2" rx="1" fill="currentColor" opacity=".18" />

		<!-- Allow pill (accent colour via SVG attribute — not CSS color:) -->
		<rect class="allow-btn" x="15" y="58" width="20" height="9" rx="4.5" fill="var(--ca-accent)" />
		<!-- Allow label squiggle -->
		<rect x="18" y="61.75" width="14" height="1.5" rx=".75" fill="var(--ca-surface-raised)" opacity=".85" />

		<!-- Deny pill -->
		<rect x="37" y="58" width="12" height="9" rx="4.5" fill="var(--ca-surface-sunken)" opacity=".8" />
		<!-- Deny label squiggle -->
		<rect x="39.5" y="61.75" width="7" height="1.5" rx=".75" fill="currentColor" opacity=".35" />
	</g>

	<!-- Tap ring — expands and fades over Allow pill -->
	<circle class="tap-ring" cx="25" cy="62.5" r="8" fill="none" stroke="var(--ca-accent)" stroke-width="1.5" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	/* Card slides up from below screen, holds, then retreats — synced to 9 s outer loop */
	.notif-card {
		animation: notif-slide 9s ease-in-out infinite;
	}
	@keyframes notif-slide {
		0%,
		25% {
			transform: translateY(24px);
			opacity: 0;
		}
		30%,
		93% {
			transform: translateY(0);
			opacity: 1;
		}
		97%,
		100% {
			transform: translateY(24px);
			opacity: 0;
		}
	}

	/* Allow button pulses while card is visible */
	.allow-btn {
		animation: btn-pulse 1.8s ease-in-out 0.5s infinite;
	}
	@keyframes btn-pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.55;
		}
	}

	/* Tap ring expands and fades at the 5–6.5 s window (55–72 % of 9 s) */
	.tap-ring {
		animation: tap-ring 9s ease-in-out infinite;
	}
	@keyframes tap-ring {
		0%,
		53% {
			opacity: 0;
			transform: scale(0.5);
		}
		58% {
			opacity: 0.9;
			transform: scale(1);
		}
		65%,
		100% {
			opacity: 0;
			transform: scale(1.8);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.notif-card {
			animation: none;
			transform: translateY(0);
			opacity: 1;
		}
		.allow-btn {
			animation: none;
		}
		.tap-ring {
			display: none;
		}
	}
</style>
