<script lang="ts">
	let { size = 80 }: { size?: number } = $props();
</script>

<!-- Celebration sparkles: 4-pointed stars that pop in and fade -->
<svg viewBox="0 0 80 80" width={size} height={size} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Each star: a 4-pointed path, staggered -->
	<path
		class="star s0"
		d="M40 10 L42.4 17.6 L50 20 L42.4 22.4 L40 30 L37.6 22.4 L30 20 L37.6 17.6 Z"
		fill="currentColor"
		opacity="0"
	/>
	<path
		class="star s1"
		d="M15 35 L16.5 40 L21 41.5 L16.5 43 L15 48 L13.5 43 L9 41.5 L13.5 40 Z"
		fill="currentColor"
		opacity="0"
	/>
	<path
		class="star s2"
		d="M65 25 L66.2 29 L70 30.2 L66.2 31.4 L65 35.4 L63.8 31.4 L60 30.2 L63.8 29 Z"
		fill="currentColor"
		opacity="0"
	/>
	<path
		class="star s3"
		d="M20 60 L21.5 64.5 L26 66 L21.5 67.5 L20 72 L18.5 67.5 L14 66 L18.5 64.5 Z"
		fill="currentColor"
		opacity="0"
	/>
	<path
		class="star s4"
		d="M60 58 L61.2 62 L65 63.2 L61.2 64.4 L60 68.4 L58.8 64.4 L55 63.2 L58.8 62 Z"
		fill="currentColor"
		opacity="0"
	/>
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}

	.star {
		animation: spark 1.6s ease-out infinite;
	}
	.s1 {
		animation-delay: 0.32s;
	}
	.s2 {
		animation-delay: 0.64s;
	}
	.s3 {
		animation-delay: 0.16s;
	}
	.s4 {
		animation-delay: 0.48s;
	}

	@keyframes spark {
		0% {
			opacity: 0;
			transform: scale(0.4) rotate(0deg);
		}
		20% {
			opacity: 0.9;
			transform: scale(1.1) rotate(15deg);
		}
		60% {
			opacity: 0.6;
			transform: scale(0.85) rotate(30deg);
		}
		100% {
			opacity: 0;
			transform: scale(0.5) rotate(45deg);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.star {
			animation: none;
			opacity: 0;
		}
		.s0 {
			opacity: 0.55;
			transform: scale(0.85);
		}
	}
</style>
