<!--
  ShipFg — reusable fg ship group: hull body, stern castle, deck rail,
  Atlas-blue waterline stripe, cannon ports, crow's-nest front rim.
  Must be placed inside a parent <svg viewBox="0 0 640 440"> at z=3
  (above crew HTML faces) so the hull occludes crew feet naturally.
  dx shifts the whole ship along X; 0 = step-1 centre (main mast x≈308).
-->
<script lang="ts">
	let {
		dx = 0,
		shipY,
		shipRot,
	}: {
		dx?: number;
		shipY: number;
		shipRot: number;
	} = $props();

	const cx = $derived(308 + dx);
</script>

<defs>
	<linearGradient id="ship-hull" x1="0" y1="0" x2="0" y2="1">
		<stop offset="0%" stop-color="#1c2c54" />
		<stop offset="100%" stop-color="#0c1730" />
	</linearGradient>
	<linearGradient id="ship-castle" x1="0" y1="0" x2="0" y2="1">
		<stop offset="0%" stop-color="#1e3062" />
		<stop offset="100%" stop-color="#162244" />
	</linearGradient>
</defs>

<!-- Hull + stern castle, bob with the same transform as the rigging layer -->
<g transform="translate({dx},{shipY}) rotate({shipRot},{cx},285)">
	<!-- Main hull body: deck y≈272, keel y≈355 → 83 px deep -->
	<path
		d="M 146 280
           Q 226 270 308 268
           Q 390 270 488 280
           L 496 292
           Q 454 325 308 330
           Q 162 325 146 294 Z"
		fill="url(#ship-hull)"
	/>

	<!-- Stern castle (raised poop deck, y 258–280) -->
	<path
		d="M 146 280
           L 146 258
           Q 180 254 228 254
           L 230 280
           Q 188 282 146 280 Z"
		fill="url(#ship-castle)"
	/>
	<line x1="228" y1="254" x2="230" y2="280" stroke="#2a3e68" stroke-width="1.5" fill="none" />

	<!-- Crow's-nest front rim in fg layer (occludes nest crew lower portion) -->
	<rect x="291" y="91" width="34" height="9" rx="2" fill="#152038" stroke="#324e82" stroke-width="1.2" />

	<!-- Atlas-blue accent stripe at waterline -->
	<path
		d="M 150 290 Q 234 286 308 284 Q 382 286 484 290"
		stroke="#3d5bff"
		stroke-width="2.8"
		fill="none"
		opacity="0.78"
	/>

	<!-- Deck surface (slightly lighter, shows sheer) -->
	<path
		d="M 146 280
           Q 226 270 308 268
           Q 390 270 488 280
           L 490 287
           Q 390 278 308 276
           Q 226 278 148 287 Z"
		fill="#1e2f5c"
	/>

	<!-- Deck rail cap -->
	<path d="M 146 280 Q 226 270 308 268 Q 390 270 488 280" stroke="#4a5e92" stroke-width="1.5" fill="none" />

	<!-- 4 cannon ports -->
	<rect x="204" y="298" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
	<rect x="270" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
	<rect x="336" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
	<rect x="402" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
</g>
