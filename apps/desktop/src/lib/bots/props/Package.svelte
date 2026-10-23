<script lang="ts">
	let { size = 36 }: { size?: number } = $props();
</script>

<!-- A small cardboard box / task package -->
<svg viewBox="0 0 40 40" width={size} height={size} aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
	<!-- Box body -->
	<rect x="4" y="16" width="32" height="22" rx="3" fill="currentColor" opacity=".85" />
	<!-- Box top flap left -->
	<path d="M4 16 L4 8 L20 8 L20 16" fill="currentColor" opacity=".65" />
	<!-- Box top flap right -->
	<path d="M20 16 L20 8 L36 8 L36 16" fill="currentColor" opacity=".5" />
	<!-- Tape strip on top -->
	<rect x="16" y="7" width="8" height="10" rx="1" fill="currentColor" opacity=".25" />
	<!-- Label on front -->
	<rect x="10" y="22" width="20" height="10" rx="2" fill="currentColor" opacity=".22" />
	<!-- Label lines -->
	<rect x="12" y="25" width="16" height="1.5" rx=".75" fill="currentColor" opacity=".35" />
	<rect x="12" y="28" width="10" height="1.5" rx=".75" fill="currentColor" opacity=".35" />
</svg>

<style>
	svg {
		display: block;
		flex: none;
	}
</style>
