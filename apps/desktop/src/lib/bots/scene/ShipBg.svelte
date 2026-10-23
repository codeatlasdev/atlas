<!--
  ShipBg — reusable bg ship group: bowsprit, masts, yards, sails, rigging,
  crow's nest interior, stern portholes, steering wheel, and lanterns.
  Must be placed inside a parent <svg viewBox="0 0 640 440">.
  dx shifts the whole ship along X; 0 = step-1 centre (main mast x≈308).
-->
<script lang="ts">
	let {
		dx = 0,
		shipY,
		shipRot,
		sailBillow,
	}: {
		dx?: number;
		shipY: number;
		shipRot: number;
		sailBillow: number;
	} = $props();

	// Per-sail billow amounts — same formula as BotArtIntro.
	const mlR = $derived(sailBillow * 30);
	const mlL = $derived(sailBillow * 6);
	const mtR = $derived(sailBillow * 16);
	const mtL = $derived(sailBillow * 3);
	const fsR = $derived(sailBillow * 26);
	const fsL = $derived(sailBillow * 5);
	const mzR = $derived(sailBillow * 20);
	const mzL = $derived(sailBillow * 4);
	const jibCtrl = $derived(sailBillow * 24);

	// Rotation centre follows the dx shift.
	const cx = $derived(308 + dx);
</script>

<defs>
	<!-- Sail: centre-bright horizontal → rounded billowed cloth -->
	<linearGradient id="ship-sail" x1="0" y1="0" x2="1" y2="0">
		<stop offset="0%" stop-color="#b4c6e6" />
		<stop offset="38%" stop-color="#ddeafe" />
		<stop offset="100%" stop-color="#c6d6ee" />
	</linearGradient>
	<linearGradient id="ship-tsail" x1="0" y1="0" x2="1" y2="0">
		<stop offset="0%" stop-color="#aebdda" />
		<stop offset="50%" stop-color="#d2e2f6" />
		<stop offset="100%" stop-color="#bfcfe8" />
	</linearGradient>
	<radialGradient id="ship-lantern" cx="50%" cy="50%" r="50%">
		<stop offset="0%" stop-color="#ffb030" stop-opacity="0.85" />
		<stop offset="60%" stop-color="#ff8820" stop-opacity="0.30" />
		<stop offset="100%" stop-color="#ff6600" stop-opacity="0" />
	</radialGradient>
</defs>

<g transform="translate({dx},{shipY}) rotate({shipRot},{cx},285)">
	<!-- Bowsprit -->
	<line x1="488" y1="288" x2="540" y2="258" stroke="#1a2848" stroke-width="6" stroke-linecap="round" />
	<line x1="488" y1="288" x2="540" y2="258" stroke="#324e82" stroke-width="2.5" stroke-linecap="round" />

	<!-- ── Masts ── -->
	<!-- Fore mast x=415 -->
	<line x1="415" y1="284" x2="415" y2="72" stroke="#1a2848" stroke-width="6" stroke-linecap="round" />
	<line x1="415" y1="284" x2="415" y2="72" stroke="#324e82" stroke-width="2.5" stroke-linecap="round" />
	<!-- Main mast x=308 -->
	<line x1="308" y1="282" x2="308" y2="50" stroke="#1a2848" stroke-width="7" stroke-linecap="round" />
	<line x1="308" y1="282" x2="308" y2="50" stroke="#324e82" stroke-width="3" stroke-linecap="round" />
	<!-- Mizzen mast x=215 -->
	<line x1="215" y1="282" x2="215" y2="82" stroke="#1a2848" stroke-width="5.5" stroke-linecap="round" />
	<line x1="215" y1="282" x2="215" y2="82" stroke="#324e82" stroke-width="2" stroke-linecap="round" />

	<!-- ── Yards (cross-spars) ── -->
	<!-- Main lower y=122 -->
	<line x1="254" y1="122" x2="362" y2="122" stroke="#1a2848" stroke-width="5.5" stroke-linecap="round" />
	<line x1="254" y1="122" x2="362" y2="122" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
	<!-- Main topsail y=78 -->
	<line x1="270" y1="78" x2="346" y2="78" stroke="#1a2848" stroke-width="4.5" stroke-linecap="round" />
	<line x1="270" y1="78" x2="346" y2="78" stroke="#324e82" stroke-width="1.8" stroke-linecap="round" />
	<!-- Fore yard y=136 -->
	<line x1="361" y1="136" x2="469" y2="136" stroke="#1a2848" stroke-width="5" stroke-linecap="round" />
	<line x1="361" y1="136" x2="469" y2="136" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
	<!-- Mizzen yard y=116 -->
	<line x1="166" y1="116" x2="264" y2="116" stroke="#1a2848" stroke-width="4.5" stroke-linecap="round" />
	<line x1="166" y1="116" x2="264" y2="116" stroke="#324e82" stroke-width="1.8" stroke-linecap="round" />

	<!-- ── Sails ── -->

	<!-- Mizzen sail -->
	<path
		d="M 166 116 L 264 116
       Q {264 + 10 + mzR} 197 {264 + 6 + mzR * 0.5} 282
       L {166 - 4 + mzL} 282
       Q {166 - 4 - mzL * 0.6} 197 166 116 Z"
		fill="url(#ship-tsail)"
		opacity="0.88"
	/>

	<!-- Main lower sail — carries the crew's emblem -->
	<path
		d="M 254 122 L 362 122
       Q {362 + 14 + mlR} 200 {362 + 8 + mlR * 0.5} 282
       L {254 - 5 + mlL} 282
       Q {254 - 5 - mlL * 0.6} 200 254 122 Z"
		fill="url(#ship-sail)"
		opacity="0.93"
	/>
	<!-- Batten lines -->
	{#each [148, 174, 200, 228, 255] as by}
		<line
			x1={254 + (by - 122) * 0.06}
			y1={by}
			x2={362 + (by - 122) * 0.15}
			y2={by}
			stroke="rgba(160,188,228,0.20)"
			stroke-width="0.9"
		/>
	{/each}

	<!-- Flag emblem on the main sail: globe + straw hat (our mark, not a skull).
	     Dark paint on light cloth; rides the sail's billow with the rest of the layer. -->
	<g transform="translate({306 + mlR * 0.35},200) scale(1.55)">
		<circle r="17" fill="#16224a" />
		<circle r="17" stroke="#3d5bff" stroke-width="1.6" fill="none" />
		<path d="M -17 0 Q -8 -3.5 0 -3.5 Q 8 -3.5 17 0" stroke="#7f95d6" stroke-width="1.3" fill="none" />
		<path d="M -17 0 Q -8 3.5 0 3.5 Q 8 3.5 17 0" stroke="#7f95d6" stroke-width="1.1" fill="none" />
		<ellipse cx="0" cy="0" rx="6.5" ry="17" stroke="#7f95d6" stroke-width="1.2" fill="none" />
		<!-- straw hat sitting on the globe -->
		<ellipse cx="0" cy="-15" rx="16" ry="4" fill="#e3b75a" stroke="#a87c2a" stroke-width="0.9" />
		<path d="M -9 -15.5 C -9 -26 9 -26 9 -15.5 Z" fill="#ecc66c" stroke="#a87c2a" stroke-width="0.9" />
		<path d="M -9 -17.5 L 9 -17.5" stroke="#3d5bff" stroke-width="2.4" />
	</g>

	<!-- Main topsail -->
	<path
		d="M 270 78 L 346 78
       Q {346 + 10 + mtR} 100 {346 + 6 + mtR * 0.5} 122
       L {270 - 4 + mtL} 122
       Q {270 - 4 - mtL * 0.5} 100 270 78 Z"
		fill="url(#ship-tsail)"
		opacity="0.88"
	/>

	<!-- Fore lower sail -->
	<path
		d="M 361 136 L 469 136
       Q {469 + 12 + fsR} 208 {469 + 6 + fsR * 0.5} 284
       L {361 - 4 + fsL} 284
       Q {361 - 4 - fsL * 0.6} 208 361 136 Z"
		fill="url(#ship-sail)"
		opacity="0.90"
	/>

	<!-- Jib (triangular, bowsprit → fore mast) -->
	<path d="M 415 72 Q {477 + jibCtrl} 162 540 257 L 415 284 Z" fill="url(#ship-tsail)" opacity="0.72" />

	<!-- ── Rigging ── -->
	<line x1="308" y1="50" x2="540" y2="257" stroke="#324e82" stroke-width="1.2" opacity="0.55" />
	<line x1="308" y1="50" x2="200" y2="282" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
	<line x1="308" y1="50" x2="415" y2="282" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
	<!-- Ratlines -->
	<line x1="275" y1="126" x2="340" y2="126" stroke="#324e82" stroke-width="0.9" opacity="0.40" />
	<line x1="256" y1="166" x2="360" y2="166" stroke="#324e82" stroke-width="0.9" opacity="0.38" />
	<line x1="238" y1="206" x2="378" y2="206" stroke="#324e82" stroke-width="0.9" opacity="0.35" />
	<line x1="220" y1="246" x2="398" y2="246" stroke="#324e82" stroke-width="0.9" opacity="0.32" />
	<!-- Fore mast stays -->
	<line x1="415" y1="72" x2="540" y2="257" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
	<line x1="415" y1="72" x2="308" y2="60" stroke="#324e82" stroke-width="0.9" opacity="0.40" />
	<line x1="415" y1="72" x2="490" y2="285" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
	<!-- Mizzen stays -->
	<line x1="215" y1="82" x2="146" y2="280" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
	<line x1="215" y1="82" x2="308" y2="282" stroke="#324e82" stroke-width="1.0" opacity="0.45" />

	<!-- ── Crow's nest (main mast, y 80–96) ── -->
	<rect x="291" y="79" width="34" height="17" rx="3" fill="#152038" stroke="#324e82" stroke-width="1.5" />
	<line x1="289" y1="96" x2="327" y2="96" stroke="#324e82" stroke-width="2" stroke-linecap="round" />

	<!-- ── Stern castle details ── -->
	<!-- Portholes -->
	<circle cx="158" cy="266" r="5.5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.3" />
	<circle cx="174" cy="266" r="5.5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.3" />
	<circle cx="190" cy="266" r="5.5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.3" />
	<circle cx="174" cy="266" r="3.8" fill="#ffa030" opacity="0.58" />
	<!-- Steering wheel -->
	<g transform="translate(192,258)">
		<circle r="12" stroke="#3e5882" stroke-width="2.2" fill="none" />
		<line x1="-12" y1="0" x2="12" y2="0" stroke="#3e5882" stroke-width="1.5" />
		<line x1="0" y1="-12" x2="0" y2="12" stroke="#3e5882" stroke-width="1.5" />
		<line x1="-8.5" y1="-8.5" x2="8.5" y2="8.5" stroke="#3e5882" stroke-width="1.5" />
		<line x1="8.5" y1="-8.5" x2="-8.5" y2="8.5" stroke="#3e5882" stroke-width="1.5" />
		<circle r="4" fill="#3e5882" />
		<circle cx="0" cy="-12" r="2.5" fill="#3e5882" />
		<circle cx="8.5" cy="-8.5" r="2.5" fill="#3e5882" />
		<circle cx="12" cy="0" r="2.5" fill="#3e5882" />
		<circle cx="8.5" cy="8.5" r="2.5" fill="#3e5882" />
		<circle cx="0" cy="12" r="2.5" fill="#3e5882" />
		<circle cx="-8.5" cy="8.5" r="2.5" fill="#3e5882" />
		<circle cx="-12" cy="0" r="2.5" fill="#3e5882" />
		<circle cx="-8.5" cy="-8.5" r="2.5" fill="#3e5882" />
	</g>
	<!-- Lanterns -->
	<circle cx="152" cy="258" r="18" fill="url(#ship-lantern)" opacity="0.7" />
	<circle cx="152" cy="258" r="5" fill="#ffa030" opacity="0.9" />
	<circle cx="164" cy="254" r="14" fill="url(#ship-lantern)" opacity="0.5" />
	<circle cx="164" cy="254" r="4" fill="#ffb840" opacity="0.85" />
</g>
