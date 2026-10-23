<!--
  BotArtServer — step 2 onboarding illustration.
  "Your hideout": Atlas Bot crew works at YOUR server (left) all night
  while your Mac sleeps (right), then a phone notification arrives.

  Story beats (~10 s intro, then calm idle loop):
    0.00–0.25  crew busy at server, route lines draw in, tasks start
    0.25–0.52  Mac lid closes + dims (night falls on that side only)
    0.52–0.75  server keeps glowing — 3 task checks complete
    0.75–1.00  phone lights up, "Reviewer finished" notification slides in

  Same craft as BotArtIntro: full-bleed 640×440 stage, 3-layer z-stack
  (bg SVG z=1 · crew HTML z=2 · fg SVG z=3), rAF clock, reduced-motion
  still frame, pauses when document is hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// ─── Stage scale ─────────────────────────────────────────────────────────
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));

	// ─── Easing ──────────────────────────────────────────────────────────────
	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// ─── Clock ───────────────────────────────────────────────────────────────
	const INTRO_MS = 10_000;
	let t = $state(reducedMotion ? 1 : 0); // 0→1 during intro, stays at 1
	let ph = $state(reducedMotion ? 1.1 : 0); // runs forever (idle heartbeat)
	let paused = $state(false);

	// ─── Stars ───────────────────────────────────────────────────────────────
	const STARS = [
		{ x: 44, y: 36, r: 1.2, p: 0.0 },
		{ x: 120, y: 20, r: 1.0, p: 1.4 },
		{ x: 204, y: 50, r: 1.3, p: 2.6 },
		{ x: 316, y: 14, r: 1.1, p: 0.8 },
		{ x: 424, y: 40, r: 1.4, p: 3.2 },
		{ x: 520, y: 18, r: 1.0, p: 2.0 },
		{ x: 570, y: 54, r: 1.2, p: 4.4 },
		{ x: 596, y: 32, r: 0.9, p: 1.6 },
	] as const;

	function starOp(phase: number): number {
		return 0.28 + 0.72 * (0.5 + 0.5 * Math.sin(ph * 1.8 + phase));
	}

	// ─── Intro beat derivations ───────────────────────────────────────────────

	// Route lines grow from server toward each device
	const routeMacPct = $derived(t < 1 ? seg(t, 0.02, 0.28) : 1);
	const routePhonePct = $derived(t < 1 ? seg(t, 0.08, 0.34) : 1);

	// Mac lid: 0 = open, 1 = closed (0° → 80° rotation around hinge)
	const macLidClose = $derived(
		t < 1 ? (t < 0.25 ? 0 : t < 0.52 ? seg(t, 0.25, 0.52) : 1) : 1,
	);
	const macLidDeg = $derived(macLidClose * 80);

	// Mac overall dim (screen + body fade as lid closes)
	const macDim = $derived(
		t < 1
			? t < 0.25
				? 1
				: t < 0.54
					? 1 - seg(t, 0.25, 0.54) * 0.76
					: 0.24
			: 0.24 + 0.05 * Math.sin(ph * 0.28),
	);

	// Server warm glow
	const serverGlow = $derived(
		t < 1 ? (t < 0.08 ? seg(t, 0, 0.08) : 1) : 0.82 + 0.18 * Math.sin(ph * 0.64),
	);

	// LED activity pulse on server rack
	const ledPulse = $derived(0.48 + 0.52 * (0.5 + 0.5 * Math.sin(ph * 2.6)));

	// Three task checks
	const check1 = $derived(t < 1 ? seg(t, 0.10, 0.22) : 1);
	const check2 = $derived(t < 1 ? seg(t, 0.38, 0.52) : 1);
	const check3 = $derived(t < 1 ? seg(t, 0.58, 0.72) : 1);

	// "working…" fades out once all checks land
	const workingOp = $derived(
		t < 1
			? t < 0.58
				? 0.65 + 0.35 * Math.abs(Math.sin(ph * 3.5))
				: 1 - seg(t, 0.58, 0.72)
			: 0,
	);

	// Moon drifts in as Mac sleeps
	const moonOp = $derived(t < 1 ? (t < 0.25 ? 0 : seg(t, 0.25, 0.54)) : 1);

	// Phone lights up and notification slides in
	const phoneLit = $derived(t < 1 ? seg(t, 0.74, 0.92) : 1);
	const notifSlide = $derived(t < 1 ? seg(t, 0.78, 0.96) : 1);

	// Globe flag wave (calm sine drift)
	const flagWave = $derived(Math.sin(ph * 1.4) * 3.5);

	// ─── Mount ───────────────────────────────────────────────────────────────
	onMount(() => {
		if (reducedMotion) return;
		const t0 = performance.now();
		let last = t0;
		let raf: number;
		const tick = (now: number) => {
			if (!paused) {
				ph += ((now - last) / 1000) * Math.PI * 0.44;
				t = Math.min(1, (now - t0) / INTRO_MS);
			}
			last = now;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			paused = document.hidden;
			if (!paused) last = performance.now();
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="ship-scene" aria-hidden="true">
	<!-- Same backdrop trick as step 1: sky colour above/below the 640×440 stage. -->
	<div class="sea" style="--sky:#08102a; --deep:#0d1f42"></div>
	<div class="stage" bind:clientWidth={stageW}>
		<!-- ═══ LAYER 1 — bg SVG (z=1): sky · stars · moon · map lines · routes ══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="1">
			<defs>
				<!-- Sky — same palette as BotArtIntro -->
				<linearGradient id="bas-sky" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#08102a" />
					<stop offset="40%" stop-color="#111e50" />
					<stop offset="78%" stop-color="#1a3272" />
					<stop offset="100%" stop-color="#1f3a7a" />
				</linearGradient>
				<!-- Server warm amber halo (left side of stage) -->
				<radialGradient id="bas-srv-halo" cx="26%" cy="60%" r="40%">
					<stop offset="0%" stop-color="#ff8c18" stop-opacity="0.30" />
					<stop offset="55%" stop-color="#ff6010" stop-opacity="0.08" />
					<stop offset="100%" stop-color="#ff6010" stop-opacity="0" />
				</radialGradient>
				<!-- Moon halo -->
				<radialGradient id="bas-moonh" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#fff8e4" stop-opacity="0.22" />
					<stop offset="55%" stop-color="#ffcc60" stop-opacity="0.08" />
					<stop offset="100%" stop-color="#ffcc60" stop-opacity="0" />
				</radialGradient>
				<!-- Ground band -->
				<linearGradient id="bas-ground" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0d1a3c" />
					<stop offset="100%" stop-color="#080f26" />
				</linearGradient>
				<!-- Clip for server→Mac route: grows left-to-right from x=258 -->
				<clipPath id="bas-clip-mac">
					<rect x="258" y="0" width={routeMacPct * 122} height="440" />
				</clipPath>
				<!-- Clip for server→Phone route: grows from x=258 -->
				<clipPath id="bas-clip-phone">
					<rect x="258" y="0" width={routePhonePct * 200} height="440" />
				</clipPath>
			</defs>

			<!-- Sky fill -->
			<rect width="640" height="440" fill="url(#bas-sky)" />

			<!-- Ground strip -->
			<rect x="0" y="372" width="640" height="68" fill="url(#bas-ground)" />
			<line x1="0" y1="372" x2="640" y2="372" stroke="rgba(55,75,135,0.30)" stroke-width="1" />

			<!-- Server warm glow (always on, pulses in idle) -->
			<rect width="640" height="440" fill="url(#bas-srv-halo)" opacity={serverGlow} />

			<!-- Stars -->
			{#each STARS as s}
				<circle cx={s.x} cy={s.y} r={s.r} fill="white" opacity={reducedMotion ? 0.55 : starOp(s.p)} />
			{/each}

			<!-- Moon — rises on Mac side as it sleeps -->
			<g transform="translate(558, 68)" opacity={moonOp}>
				<circle r="52" fill="url(#bas-moonh)" />
				<circle r="20" fill="#fff5da" />
				<circle cx="5" cy="-4" r="4.5" fill="rgba(196,172,108,0.18)" />
				<circle cx="-7" cy="6" r="3" fill="rgba(196,172,108,0.13)" />
			</g>

			<!-- Nautical map texture: faint rhumb lines radiating from server -->
			<g stroke="rgba(70,100,190,0.055)" stroke-width="0.9" fill="none">
				<line x1="160" y1="270" x2="640" y2="110" />
				<line x1="160" y1="270" x2="620" y2="440" />
				<line x1="160" y1="270" x2="420" y2="28" />
				<line x1="160" y1="270" x2="640" y2="340" />
				<line x1="160" y1="270" x2="640" y2="220" />
			</g>

			<!-- ── Connection route: server → Mac ──────────────────────────── -->
			<path
				d="M 260 210 Q 312 160 368 188"
				stroke="rgba(110,170,255,0.42)"
				stroke-width="1.8"
				stroke-dasharray="6 9"
				fill="none"
				clip-path="url(#bas-clip-mac)"
			/>
			{#if routeMacPct > 0.04}
				<circle cx="260" cy="210" r="3.2" fill="rgba(110,170,255,0.55)" />
			{/if}
			{#if routeMacPct > 0.94}
				<circle cx="368" cy="188" r="3.2" fill="rgba(110,170,255,0.55)" />
			{/if}

			<!-- ── Connection route: server → Phone ────────────────────────── -->
			<path
				d="M 260 252 Q 336 308 456 326"
				stroke="rgba(80,155,255,0.38)"
				stroke-width="1.8"
				stroke-dasharray="6 9"
				fill="none"
				clip-path="url(#bas-clip-phone)"
			/>
			{#if routePhonePct > 0.94}
				<circle cx="456" cy="326" r="3.2" fill="rgba(80,155,255,0.55)" />
			{/if}

			<!-- ── Compass rose (bottom-left) ──────────────────────────────── -->
			<g transform="translate(44, 402)" opacity="0.20">
				<circle r="16" fill="none" stroke="rgba(130,160,255,0.55)" stroke-width="0.8" />
				<line x1="-16" y1="0" x2="16" y2="0" stroke="rgba(130,160,255,0.7)" stroke-width="0.8" />
				<line x1="0" y1="-16" x2="0" y2="16" stroke="rgba(130,160,255,0.7)" stroke-width="0.8" />
				<!-- N pointer in Atlas blue -->
				<polygon points="0,-9 2.5,-4 0,-6.5 -2.5,-4" fill="#4f8ef7" />
				<polygon points="0,9 2.5,4 0,6.5 -2.5,4" fill="rgba(130,160,255,0.45)" />
			</g>
		</svg>

		<!-- ═══ LAYER 2 — crew HTML (z=2): three BotFaces at the server ═════════
             Feet (bottom edge) land just inside the server top panel at y≈242;
             the fg SVG server face (z=3) covers the bottom ~10 px of each face.
		-->
		<div class="crew-layer">
			<!-- A: centre — main operator at the server console -->
			<div
				class="crew-face"
				style:left="22.7%"
				style:top="55.0%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="srv-a" state="working" size={Math.round(58 * k)} />
			</div>
			<!-- B: left — monitoring the rack LEDs -->
			<div
				class="crew-face"
				style:left="13.3%"
				style:top="57.3%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="srv-b" state="working" size={Math.round(52 * k)} />
			</div>
			<!-- C: right — watching the task log -->
			<div
				class="crew-face"
				style:left="34.1%"
				style:top="54.5%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="srv-c" state="working" size={Math.round(50 * k)} />
			</div>
		</div>

		<!-- ═══ LAYER 3 — fg SVG (z=3): server · flag · task log · Mac · phone ═══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="3">
			<defs>
				<!-- Server rack body -->
				<linearGradient id="bas-srv-body" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1c2c56" />
					<stop offset="100%" stop-color="#0e1b38" />
				</linearGradient>
				<!-- Server rack unit slot -->
				<linearGradient id="bas-rack-unit" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#233068" />
					<stop offset="65%" stop-color="#1c2852" />
					<stop offset="100%" stop-color="#172040" />
				</linearGradient>
				<!-- Server front amber glow (inner warmth) -->
				<radialGradient id="bas-srv-inner" cx="50%" cy="0%" r="70%">
					<stop offset="0%" stop-color="#ff8820" stop-opacity="0.14" />
					<stop offset="100%" stop-color="#ff8820" stop-opacity="0" />
				</radialGradient>
				<!-- Small screen on rack unit 1 -->
				<linearGradient id="bas-srv-screen" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0b1528" />
					<stop offset="100%" stop-color="#060e1e" />
				</linearGradient>
				<!-- Mac lid screen -->
				<linearGradient id="bas-mac-screen" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1a2b58" />
					<stop offset="100%" stop-color="#101d40" />
				</linearGradient>
				<!-- Mac base body -->
				<linearGradient id="bas-mac-base" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1e2a50" />
					<stop offset="100%" stop-color="#141d3a" />
				</linearGradient>
				<!-- Phone screen -->
				<linearGradient id="bas-phone-screen" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#19295a" />
					<stop offset="100%" stop-color="#0d1838" />
				</linearGradient>
				<!-- Phone glow on notification -->
				<radialGradient id="bas-phone-glow" cx="50%" cy="50%" r="55%">
					<stop offset="0%" stop-color="#4f8ef7" stop-opacity="0.45" />
					<stop offset="100%" stop-color="#4f8ef7" stop-opacity="0" />
				</radialGradient>
			</defs>

			<!-- ══ SERVER RACK ════════════════════════════════════════════════════ -->
			<!-- Outer casing -->
			<rect x="60" y="234" width="202" height="138" rx="6" fill="url(#bas-srv-body)" stroke="#2e3e70" stroke-width="1.5" />
			<!-- Inner warmth overlay -->
			<rect x="68" y="242" width="186" height="122" rx="4" fill="url(#bas-srv-inner)" />
			<!-- Atlas-blue accent stripe at top of rack -->
			<rect x="62" y="234" width="200" height="3.5" rx="1.5" fill="#3d5bff" opacity="0.88" />

			<!-- Rack unit 1 — console screen + activity -->
			<rect x="70" y="244" width="184" height="28" rx="3" fill="url(#bas-rack-unit)" />
			<!-- Small display -->
			<rect x="76" y="249" width="104" height="18" rx="2" fill="url(#bas-srv-screen)" />
			<!-- Pulsing green activity dot -->
			<circle cx="83" cy="258" r="3.6" fill="#22c55e" opacity={workingOp > 0 ? workingOp * ledPulse : ledPulse * 0.4} />
			<!-- "working…" -->
			<text x="91" y="262" fill="rgba(134,239,172,0.88)" font-size="9" font-family="monospace" opacity={workingOp}>working…</text>
			<!-- Drive activity LEDs (right of unit 1) -->
			<circle cx="216" cy="252" r="2.8" fill="#22c55e" opacity={ledPulse} />
			<circle cx="226" cy="252" r="2.8" fill="#22c55e" opacity={ledPulse * 0.55} />
			<circle cx="236" cy="252" r="2.8" fill="#f97316" opacity={ledPulse * 0.80} />

			<!-- Rack unit separator -->
			<line x1="68" y1="273" x2="260" y2="273" stroke="#1a2448" stroke-width="1" />

			<!-- Rack unit 2 — drive bays + power LED -->
			<rect x="70" y="275" width="184" height="28" rx="3" fill="url(#bas-rack-unit)" />
			<rect x="76" y="281" width="118" height="6" rx="2" fill="rgba(26,36,72,0.8)" />
			<rect x="76" y="291" width="118" height="6" rx="2" fill="rgba(26,36,72,0.8)" />
			<circle cx="208" cy="284" r="2.5" fill="#22c55e" opacity={ledPulse * 0.9} />
			<circle cx="208" cy="294" r="2.5" fill="#22c55e" opacity={ledPulse * 0.5} />
			<circle cx="220" cy="284" r="2.5" fill="#22c55e" opacity={0.5 + 0.5 * Math.sin(ph * 1.1)} />
			<circle cx="232" cy="284" r="2.5" fill="#f97316" opacity="0.6" />
			<circle cx="244" cy="284" r="2.5" fill="rgba(100,120,200,0.4)" />

			<!-- Rack unit separator -->
			<line x1="68" y1="304" x2="260" y2="304" stroke="#1a2448" stroke-width="1" />

			<!-- Rack unit 3 — more bays -->
			<rect x="70" y="306" width="184" height="28" rx="3" fill="url(#bas-rack-unit)" />
			<rect x="76" y="312" width="95" height="6" rx="2" fill="rgba(26,36,72,0.8)" />
			<rect x="76" y="322" width="70" height="6" rx="2" fill="rgba(26,36,72,0.8)" />
			<circle cx="208" cy="315" r="2.5" fill="#22c55e" opacity={ledPulse * 0.7} />
			<circle cx="220" cy="315" r="2.5" fill="#22c55e" opacity={ledPulse * 0.4} />

			<!-- Rack unit separator -->
			<line x1="68" y1="335" x2="260" y2="335" stroke="#1a2448" stroke-width="1" />

			<!-- Rack unit 4 — bottom strip + power button -->
			<rect x="70" y="337" width="184" height="24" rx="3" fill="url(#bas-rack-unit)" />
			<circle cx="80" cy="349" r="5" fill="#0e1630" stroke="#22c55e" stroke-width="1.2" />
			<circle cx="80" cy="349" r="2.5" fill="#22c55e" opacity={0.6 + 0.4 * Math.sin(ph * 0.7)} />

			<!-- Rack top highlight edge -->
			<path d="M 64 234 Q 161 230 260 234" stroke="#4f6aac" stroke-width="1" fill="none" opacity="0.55" />
			<!-- Rack bottom edge shadow -->
			<path d="M 64 372 Q 161 374 260 372" stroke="#080f22" stroke-width="2" fill="none" opacity="0.6" />

			<!-- ══ GLOBE + STRAW-HAT FLAG ════════════════════════════════════════ -->
			<!-- Planted on the left corner of the server top — "claimed island" -->
			<!-- Pole -->
			<line x1="76" y1="185" x2="76" y2="234" stroke="#8aa0cc" stroke-width="1.6" />
			<!-- Flag cloth (waves with ph) -->
			<path
				d="M 76 185
           L 76 198
           Q {88 + flagWave} 195 {100 + flagWave} 192
           Q {88 + flagWave * 0.7} 189 76 185 Z"
				fill="#3d5bff"
				opacity="0.90"
			/>
			<!-- Globe on pole tip -->
			<circle cx="76" cy="181" r="9.5" fill="#1a3272" stroke="#3d5bff" stroke-width="1.2" />
			<!-- Meridian lines on globe -->
			<ellipse cx="76" cy="181" rx="4.5" ry="9.5" fill="none" stroke="#4f8ef7" stroke-width="0.7" opacity="0.65" />
			<line x1="66.5" y1="181" x2="85.5" y2="181" stroke="#4f8ef7" stroke-width="0.7" opacity="0.65" />
			<!-- Straw hat on globe -->
			<ellipse cx="76" cy="173.5" rx="8" ry="2.4" fill="#d4a827" />
			<ellipse cx="76" cy="172.5" rx="4.5" ry="3.4" fill="#b88820" />
			<!-- Hat brim shadow line -->
			<path d="M 68 174.5 Q 76 177 84 174.5" stroke="#9a7218" stroke-width="0.9" fill="none" opacity="0.7" />

			<!-- ══ TASK LOG — floats between server and Mac, above crew ═══════════ -->
			<!-- Subtle dark backdrop panel -->
			<rect x="280" y="138" width="100" height="68" rx="5" fill="rgba(8,14,32,0.72)" stroke="rgba(60,90,180,0.28)" stroke-width="0.8" />
			<!-- Task 1: tests passing -->
			<g opacity={check1}>
				<circle cx="293" cy="155" r="6.5" fill="#14522e" stroke="#22c55e" stroke-width="1" />
				<path d="M 289.5 155 L 292.5 158.5 L 297.5 151.5" stroke="#22c55e" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
				<text x="305" y="159" fill="rgba(134,239,172,0.82)" font-size="9.5" font-family="monospace">tests passing</text>
			</g>
			<!-- Task 2: PR reviewed -->
			<g opacity={check2}>
				<circle cx="293" cy="175" r="6.5" fill="#14522e" stroke="#22c55e" stroke-width="1" />
				<path d="M 289.5 175 L 292.5 178.5 L 297.5 171.5" stroke="#22c55e" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
				<text x="305" y="179" fill="rgba(134,239,172,0.82)" font-size="9.5" font-family="monospace">PR reviewed</text>
			</g>
			<!-- Task 3: build done -->
			<g opacity={check3}>
				<circle cx="293" cy="195" r="6.5" fill="#14522e" stroke="#22c55e" stroke-width="1" />
				<path d="M 289.5 195 L 292.5 198.5 L 297.5 191.5" stroke="#22c55e" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round" />
				<text x="305" y="199" fill="rgba(134,239,172,0.82)" font-size="9.5" font-family="monospace">build done</text>
			</g>

			<!-- ══ MAC LAPTOP ════════════════════════════════════════════════════ -->
			<g opacity={macDim}>
				<!-- Base / keyboard chassis -->
				<rect x="364" y="268" width="172" height="82" rx="6" fill="url(#bas-mac-base)" stroke="#2a3860" stroke-width="1.2" />
				<!-- Hinge face -->
				<rect x="368" y="268" width="164" height="5" rx="1.5" fill="#1a2244" />
				<!-- Keyboard suggestion — 3 rows of key caps -->
				{#each [0, 1, 2] as row}
					{#each [0, 1, 2, 3, 4, 5, 6] as col}
						<rect
							x={374 + col * 22}
							y={280 + row * 15}
							width="18"
							height="11"
							rx="2"
							fill="rgba(70,90,150,0.16)"
						/>
					{/each}
				{/each}
				<!-- Trackpad -->
				<rect x="415" y="335" width="70" height="44" rx="5" fill="rgba(50,70,130,0.18)" stroke="rgba(70,95,155,0.18)" stroke-width="0.8" />
				<!-- Bottom chin -->
				<rect x="368" y="346" width="164" height="4" rx="1.5" fill="#131b36" />

				<!-- Lid / screen — rotates clockwise around hinge (450, 268) to close -->
				<!-- At macLidDeg=0: screen is open (facing us, extending upward).    -->
				<!-- At macLidDeg=80: screen has swung ~flat onto keyboard.           -->
				<g transform="rotate({macLidDeg}, 450, 268)">
					<!-- Lid frame -->
					<rect x="368" y="95" width="164" height="175" rx="8" fill="url(#bas-mac-base)" stroke="#2a3860" stroke-width="1.2" />
					<!-- Screen surface -->
					<rect x="378" y="104" width="144" height="157" rx="5" fill="url(#bas-mac-screen)" />
					<!-- Atlas-blue top bar accent -->
					<rect x="378" y="104" width="144" height="3.5" rx="1.5" fill="#3d5bff" opacity="0.55" />
					<!-- Fake UI — menu bar -->
					<rect x="383" y="112" width="134" height="9" rx="2" fill="rgba(55,75,140,0.38)" />
					<!-- Fake content lines -->
					<rect x="386" y="129" width="78" height="5" rx="1.5" fill="rgba(70,110,190,0.24)" />
					<rect x="386" y="140" width="108" height="5" rx="1.5" fill="rgba(70,110,190,0.18)" />
					<rect x="386" y="151" width="58" height="5" rx="1.5" fill="rgba(70,110,190,0.20)" />
					<rect x="386" y="162" width="92" height="5" rx="1.5" fill="rgba(70,110,190,0.16)" />
					<rect x="386" y="173" width="42" height="5" rx="1.5" fill="rgba(70,110,190,0.18)" />
					<rect x="386" y="184" width="124" height="5" rx="1.5" fill="rgba(70,110,190,0.14)" />
					<!-- Camera dot -->
					<circle cx="450" cy="100" r="2.5" fill="rgba(30,45,80,0.7)" />
				</g>
			</g>

			<!-- ══ PHONE ═════════════════════════════════════════════════════════ -->
			<!-- Body always subtly visible; glows when notification arrives -->
			<rect x="456" y="315" width="68" height="110" rx="10" fill="#131c36" stroke="#252e55" stroke-width="1.2" opacity={0.25 + phoneLit * 0.55} />
			<!-- Screen -->
			<rect x="463" y="325" width="54" height="86" rx="6" fill="url(#bas-phone-screen)" opacity={0.2 + phoneLit * 0.75} />
			<!-- Phone glow backdrop on notification -->
			<rect x="456" y="315" width="68" height="110" rx="10" fill="url(#bas-phone-glow)" opacity={phoneLit * 0.55} />
			<!-- Notch -->
			<rect x="475" y="317" width="30" height="5" rx="2.5" fill="#0a1228" opacity={0.3 + phoneLit * 0.4} />

			<!-- Notification card — slides up into view -->
			<g transform="translate(0, {(1 - notifSlide) * 20})" opacity={notifSlide}>
				<!-- Card background -->
				<rect x="467" y="334" width="46" height="46" rx="5" fill="#1a2e5c" stroke="#3d5bff" stroke-width="0.8" />
				<!-- App icon (square with Atlas blue) -->
				<rect x="470" y="337" width="13" height="13" rx="3" fill="#3d5bff" />
				<!-- Tiny globe on icon -->
				<circle cx="476.5" cy="343.5" r="4" fill="#1a3272" />
				<line x1="472.5" y1="343.5" x2="480.5" y2="343.5" stroke="#4f8ef7" stroke-width="0.6" opacity="0.7" />
				<ellipse cx="476.5" cy="343.5" rx="2" ry="4" fill="none" stroke="#4f8ef7" stroke-width="0.6" opacity="0.7" />
				<!-- Tiny straw hat on icon -->
				<ellipse cx="476.5" cy="339.8" rx="4.5" ry="1.4" fill="#d4a827" />
				<ellipse cx="476.5" cy="339.2" rx="2.5" ry="2" fill="#b88820" />
				<!-- "Atlas Bot" header -->
				<text x="486" y="346" fill="rgba(170,195,255,0.88)" font-size="7" font-family="inherit" font-weight="600">Atlas Bot</text>
				<!-- Notification body -->
				<text x="470" y="360" fill="rgba(134,239,172,0.82)" font-size="7.5" font-family="inherit">Reviewer</text>
				<text x="470" y="371" fill="rgba(134,239,172,0.72)" font-size="7" font-family="inherit">finished</text>
				<!-- Checkmark icon next to "finished" -->
				<circle cx="503" cy="368" r="5" fill="#14522e" stroke="#22c55e" stroke-width="0.8" />
				<path d="M 500.5 368 L 502.5 370.5 L 506 365.5" stroke="#22c55e" stroke-width="1.2" fill="none" stroke-linecap="round" stroke-linejoin="round" />
			</g>
		</svg>
	</div>
</div>

<style>
	.ship-scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		container-type: size;
	}

	/* Continues the sky colour above the stage and deep colour below it,
	   so the scene fills the whole column without a hard edge. */
	.sea {
		position: absolute;
		inset: 0;
		background: linear-gradient(
			to bottom,
			var(--sky) calc(50% - 34.375cqw + 1px),
			transparent calc(50% - 34.375cqw + 1px),
			transparent calc(50% + 34.375cqw - 1px),
			var(--deep) calc(50% + 34.375cqw - 1px)
		);
	}

	/* Fixed-ratio stage: all % positions align with the SVG 640×440 viewBox. */
	.stage {
		position: relative;
		width: 100cqw;
		aspect-ratio: 640 / 440;
		flex-shrink: 0;
	}

	.layer-svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		overflow: visible;
		pointer-events: none;
	}

	/* z-index:2 puts the crew ABOVE bg SVG elements (z=1)
	   and BELOW fg SVG elements that occlude their feet (z=3). */
	.crew-layer {
		position: absolute;
		inset: 0;
		z-index: 2;
		pointer-events: none;
		will-change: transform;
	}

	.crew-face {
		position: absolute;
	}
</style>
