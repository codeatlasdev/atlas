<script lang="ts">
	let { size = 64 }: { size?: number } = $props();

	const w = $derived(size);
	const h = $derived((size * 110) / 64);
</script>

<!-- Phone with a sliding notification card and tap animation -->
<svg viewBox="0 0 64 110" width={w} height={h} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Phone body -->
	<rect x="8" y="4" width="48" height="90" rx="10" fill="currentColor" opacity=".85" />
	<!-- Screen -->
	<rect x="12" y="12" width="40" height="74" rx="6" fill="currentColor" opacity=".45" />
	<!-- Status bar -->
	<rect x="18" y="15" width="28" height="3" rx="1.5" fill="currentColor" opacity=".25" />
	<!-- Home indicator -->
	<rect x="24" y="82" width="16" height="3" rx="1.5" fill="currentColor" opacity=".35" />
	<!-- Notch -->
	<rect x="26" y="10" width="12" height="4" rx="2" fill="currentColor" opacity=".6" />

	<!-- Notification card — slides up then holds -->
	<g class="notif">
		<rect x="14" y="28" width="36" height="28" rx="6" fill="var(--ca-surface-raised, #fff)" opacity=".95" />
		<rect
			x="14"
			y="28"
			width="36"
			height="28"
			rx="6"
			stroke="currentColor"
			stroke-width=".75"
			fill="none"
			opacity=".2"
		/>
		<!-- App icon dot -->
		<circle cx="22" cy="37" r="4" fill="currentColor" opacity=".7" />
		<!-- Title line -->
		<rect x="29" y="33" width="16" height="2.5" rx="1.25" fill="currentColor" opacity=".5" />
		<!-- Body line -->
		<rect x="29" y="38" width="12" height="2" rx="1" fill="currentColor" opacity=".3" />
		<!-- Allow button -->
		<rect class="allow-btn" x="18" y="48" width="28" height="6" rx="3" fill="currentColor" opacity=".25" />
		<rect
			class="allow-btn"
			x="18"
			y="48"
			width="28"
			height="6"
			rx="3"
			stroke="currentColor"
			stroke-width=".75"
			fill="none"
			opacity=".45"
		/>
	</g>

	<!-- Finger tap hint — a circle that pulses and fades at the button -->
	<circle class="tap" cx="32" cy="51" r="6" fill="currentColor" opacity="0" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	/* Slide in from below, hold, slide out, gap */
	.notif {
		animation: notif-slide 6s ease-in-out infinite;
	}
	@keyframes notif-slide {
		0% {
			transform: translateY(20px);
			opacity: 0;
		}
		12%,
		70% {
			transform: translateY(0);
			opacity: 1;
		}
		82%,
		100% {
			transform: translateY(20px);
			opacity: 0;
		}
	}

	/* Button pulses while notification is visible */
	.allow-btn {
		animation: btn-pulse 1.8s ease-in-out 0.8s infinite;
	}
	@keyframes btn-pulse {
		0%,
		100% {
			opacity: 0.25;
		}
		50% {
			opacity: 0.7;
		}
	}

	/* Tap: appear at peak of button pulse then fade */
	.tap {
		animation: tap-pulse 6s ease-in-out infinite;
	}
	@keyframes tap-pulse {
		0%,
		44% {
			opacity: 0;
			transform: scale(0.5);
		}
		50% {
			opacity: 0.55;
			transform: scale(1);
		}
		58%,
		100% {
			opacity: 0;
			transform: scale(1.6);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.notif {
			animation: none;
			transform: translateY(0);
			opacity: 1;
		}
		.allow-btn {
			animation: none;
		}
		.tap {
			display: none;
		}
	}
</style>
