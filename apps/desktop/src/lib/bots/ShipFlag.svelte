<script lang="ts">
	let { size = 32 }: { size?: number } = $props();
</script>

<!--
	ShipFlag — Atlas globe with a straw hat.
	Monochrome (currentColor) with accent-color band.
-->
<svg
	width={size}
	height={size}
	viewBox="0 0 32 32"
	fill="none"
	xmlns="http://www.w3.org/2000/svg"
	aria-hidden="true"
>
	<!-- Globe circle -->
	<circle cx="16" cy="20" r="9" stroke="currentColor" stroke-width="1.5" fill="none" />

	<!-- Globe longitude arcs (two crossing arcs) -->
	<path d="M16 11 C12 13 10 17 10 20 C10 23 12 27 16 29" stroke="currentColor" stroke-width="1" fill="none" />
	<path d="M16 11 C20 13 22 17 22 20 C22 23 20 27 16 29" stroke="currentColor" stroke-width="1" fill="none" />
	<!-- Globe latitude line -->
	<path d="M7.2 20 Q16 17.5 24.8 20" stroke="currentColor" stroke-width="1" fill="none" />

	<!-- Hat brim (wide flat ellipse) -->
	<ellipse cx="16" cy="13.5" rx="10" ry="2.2" fill="currentColor" opacity="0.15" />
	<ellipse cx="16" cy="13.5" rx="10" ry="2.2" stroke="currentColor" stroke-width="1.25" fill="none" />

	<!-- Hat crown (dome) -->
	<path d="M9 13.5 C9 13.5 9 7 16 7 C23 7 23 13.5 23 13.5" fill="currentColor" opacity="0.12" />
	<path
		d="M9 13.5 C9 13.5 9 7 16 7 C23 7 23 13.5 23 13.5"
		stroke="currentColor"
		stroke-width="1.25"
		fill="none"
	/>

	<!-- Hat band (accent color) -->
	<path
		d="M9.5 12 C11.5 11.2 14 10.8 16 10.8 C18 10.8 20.5 11.2 22.5 12"
		stroke="var(--color-accent, #3D5BFF)"
		stroke-width="2"
		stroke-linecap="round"
		fill="none"
	/>

	<!-- Meridian arc on crown -->
	<path
		d="M16 7 C14.2 8.5 13.5 10.5 13.5 12.5"
		stroke="currentColor"
		stroke-width="0.75"
		fill="none"
		opacity="0.5"
	/>
</svg>
