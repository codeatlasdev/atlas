<script lang="ts">
	let { size = 40 }: { size?: number } = $props();
</script>

<!-- Coffee mug with animated steam -->
<svg
	viewBox="0 0 56 72"
	width={size}
	height={(size * 72) / 56}
	aria-hidden="true"
	xmlns="http://www.w3.org/2000/svg"
>
	<!-- Steam wisps — opacity/transform only -->
	<path
		class="steam s1"
		d="M16 24 Q12 17 16 10 Q20 3 16 -3"
		stroke="currentColor"
		stroke-width="2.5"
		fill="none"
		stroke-linecap="round"
		opacity=".0"
	/>
	<path
		class="steam s2"
		d="M28 22 Q24 15 28 8 Q32 1 28 -5"
		stroke="currentColor"
		stroke-width="2.5"
		fill="none"
		stroke-linecap="round"
		opacity=".0"
	/>
	<path
		class="steam s3"
		d="M40 24 Q36 17 40 10 Q44 3 40 -3"
		stroke="currentColor"
		stroke-width="2.5"
		fill="none"
		stroke-linecap="round"
		opacity=".0"
	/>

	<!-- Cup body -->
	<rect x="6" y="28" width="38" height="34" rx="6" fill="currentColor" opacity=".9" />
	<!-- Handle -->
	<path
		d="M44 36 Q58 42 44 52"
		stroke="currentColor"
		stroke-width="5"
		fill="none"
		stroke-linecap="round"
		opacity=".9"
	/>
	<!-- Rim -->
	<rect x="4" y="23" width="42" height="9" rx="4" fill="currentColor" opacity=".75" />
	<!-- Liquid surface highlight -->
	<ellipse cx="25" cy="27" rx="16" ry="3.5" fill="currentColor" opacity=".35" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	.steam {
		animation: steam-rise 3s ease-out infinite;
	}
	.s2 {
		animation-delay: 1s;
	}
	.s3 {
		animation-delay: 2s;
	}

	@keyframes steam-rise {
		0% {
			opacity: 0;
			transform: translateY(0);
		}
		25% {
			opacity: 0.55;
			transform: translateY(-4px);
		}
		100% {
			opacity: 0;
			transform: translateY(-14px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.steam {
			animation: none;
			opacity: 0;
		}
	}
</style>
