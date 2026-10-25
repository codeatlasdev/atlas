<!--
  BotArtIntro — step 1 onboarding illustration.
  Pirate tall ship: 3 masts, square sails, crow's nest, stern castle.
  5 BotFace crew clearly above the hull rail.
  Z-stack: bg SVG (z=1) sky/rigging/sails → crew HTML (z=2) → fg SVG (z=3) hull+waves.
  10 s scripted intro then calm idle.  Reduced-motion → still frame.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// ─── Wave geometry ──────────────────────────────────────────────────────────
	// 16 half-periods × 80 px = 1280 px (2 × viewBox width).
	// CSS animation shifts −640 px → seamless tile.

	function waveD(y: number, amp: number): string {
		const pts = [`M 0 ${y}`];
		for (let i = 0; i < 16; i++) {
			const x0 = i * 80;
			pts.push(`Q ${x0 + 40} ${i % 2 === 0 ? y - amp : y + amp} ${x0 + 80} ${y}`);
		}
		pts.push(`L 1280 440 L 0 440 Z`);
		return pts.join(" ");
	}

	function waveLine(y: number, amp: number): string {
		const pts = [`M 0 ${y}`];
		for (let i = 0; i < 16; i++) {
			const x0 = i * 80;
			pts.push(`Q ${x0 + 40} ${i % 2 === 0 ? y - amp : y + amp} ${x0 + 80} ${y}`);
		}
		return pts.join(" ");
	}

	// All waves below hull waterline (y ≥ 300)
	const W4 = waveD(300, 4);
	const W3 = waveD(316, 8);
	const W2 = waveD(336, 12);
	const W1 = waveD(356, 17);
	const W1L = waveLine(356, 17);
	const W2L = waveLine(336, 12);

	// ─── Animation state ────────────────────────────────────────────────────────

	const INTRO_MS = 10_000;
	let t = $state(reducedMotion ? 1 : 0);
	let ph = $state(reducedMotion ? 1.1 : 0);
	let paused = $state(false);

	// ─── Easing ─────────────────────────────────────────────────────────────────

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	function spring(x: number, a: number, b: number): number {
		const raw = (x - a) / (b - a);
		if (raw <= 0) return 0;
		const p = Math.min(1, raw);
		const base = p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
		if (raw <= 1) return base;
		return 1 + Math.sin(((raw - 1) / 0.2) * Math.PI) * 0.11;
	}

	// ─── Ship dynamics ──────────────────────────────────────────────────────────

	// Deck centre (308, 282) — two offset frequencies → overlapping action
	const shipY = $derived(Math.sin(ph) * 7);
	const shipRot = $derived(Math.cos(ph * 1.07) * 2.2);

	// The stage scales with the column (640×440 is its design size); the crew are HTML faces with a px size,
	// so they scale by the same factor or they shrink next to a bigger ship.
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));

	// ─── Sail billow (0 = flat baseline, 1 = full wind) ─────────────────────────
	// Sails always have a 10–14 px baseline bow-forward curve so they read as
	// rounded cloth even before the intro beat runs.

	const sailBillow = $derived(
		t < 1
			? Math.max(0, spring(t, 0.05, 0.28))
			: 0.88 + Math.sin(ph * 0.58) * 0.07 + Math.sin(ph * 1.31) * 0.03,
	);

	// Per-sail additional billow on top of the permanent baseline
	const mlR = $derived(sailBillow * 30); // main lower, right (forward) edge extra
	const mlL = $derived(sailBillow * 6); // left (aft) edge extra concave
	const mtR = $derived(sailBillow * 16);
	const mtL = $derived(sailBillow * 3);
	const fsR = $derived(sailBillow * 26);
	const fsL = $derived(sailBillow * 5);
	const mzR = $derived(sailBillow * 20);
	const mzL = $derived(sailBillow * 4);
	const jibCtrl = $derived(sailBillow * 24);

	// ─── FX ─────────────────────────────────────────────────────────────────────

	const routeW = $derived(t < 1 ? seg(t, 0.42, 0.8) * 175 : 175);
	const compassOp = $derived(t < 1 ? seg(t, 0.65, 0.92) * 0.28 : 0.28);
	const moonOp = $derived(t < 1 ? seg(t, 0, 0.22) : 1);

	const STARS = [
		{ x: 62, y: 42, r: 1.4, p: 0.0 },
		{ x: 138, y: 24, r: 1.1, p: 1.3 },
		{ x: 222, y: 54, r: 1.5, p: 2.5 },
		{ x: 302, y: 14, r: 1.2, p: 0.7 },
		{ x: 382, y: 36, r: 1.3, p: 3.1 },
		{ x: 458, y: 18, r: 1.0, p: 1.9 },
		{ x: 518, y: 50, r: 1.4, p: 4.3 },
		{ x: 576, y: 28, r: 1.1, p: 2.8 },
	] as const;

	function starOp(phase: number): number {
		return 0.28 + 0.72 * (0.5 + 0.5 * Math.sin(ph * 1.8 + phase));
	}

	function gullPos(
		iS: number,
		iE: number,
		off: number,
		baseY: number,
	): { x: number; y: number; vis: boolean } {
		let p = -1;
		if (t < 1) p = seg(t, iS, iE);
		else {
			const c = ((ph * 0.19 + off) % (Math.PI * 2)) / (Math.PI * 2);
			p = c < 0.38 ? c / 0.38 : -1;
		}
		if (p < 0 || p > 1) return { x: -60, y: baseY, vis: false };
		return { x: 700 - p * 770, y: baseY + Math.sin(p * Math.PI * 2.2) * 14, vis: true };
	}

	const gull1 = $derived(gullPos(0.48, 0.68, 0, 80));
	const gull2 = $derived(gullPos(0.52, 0.72, 2.1, 65));

	const crew1State = $derived(
		!reducedMotion && t > 0.18 && t < 0.6 ? ("needs_you" as const) : ("idle" as const),
	);

	// ─── Mount ──────────────────────────────────────────────────────────────────

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
	<!-- Sky above the stage and sea below it, so the scene fills the whole column: the stage spans the full width and
	     this layer continues its top colour (the sky's) up and its front wave's colour down. -->
	<div class="sea" style="--sky:#08102a; --deep:#1b3e7a"></div>
	<div class="stage" bind:clientWidth={stageW}>
		<!-- ═══ LAYER 1 — bg SVG (z=1): sky · water · rigging · sails ═══════════ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="1">
			<defs>
				<linearGradient id="bai-sky" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#08102a" />
					<stop offset="40%" stop-color="#111e50" />
					<stop offset="78%" stop-color="#1a3272" />
					<stop offset="100%" stop-color="#1f3a7a" />
				</linearGradient>
				<radialGradient id="bai-glow" cx="50%" cy="100%" r="68%">
					<stop offset="0%" stop-color="#cc5f18" stop-opacity="0.44" />
					<stop offset="55%" stop-color="#cc5f18" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#cc5f18" stop-opacity="0" />
				</radialGradient>
				<radialGradient id="bai-moonh" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#fff8e4" stop-opacity="0.28" />
					<stop offset="55%" stop-color="#ffcc60" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#ffcc60" stop-opacity="0" />
				</radialGradient>
				<linearGradient id="bai-water" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0a1e42" />
					<stop offset="100%" stop-color="#050e1e" />
				</linearGradient>
				<!-- Sail: centre-bright horizontal → suggests rounded billowed cloth -->
				<linearGradient id="bai-sail" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#b4c6e6" />
					<stop offset="38%" stop-color="#ddeafe" />
					<stop offset="100%" stop-color="#c6d6ee" />
				</linearGradient>
				<linearGradient id="bai-tsail" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#aebdda" />
					<stop offset="50%" stop-color="#d2e2f6" />
					<stop offset="100%" stop-color="#bfcfe8" />
				</linearGradient>
				<linearGradient id="bai-refl" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#ffcc60" stop-opacity="0.22" />
					<stop offset="100%" stop-color="#ffcc60" stop-opacity="0" />
				</linearGradient>
				<radialGradient id="bai-lantern" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#ffb030" stop-opacity="0.85" />
					<stop offset="60%" stop-color="#ff8820" stop-opacity="0.30" />
					<stop offset="100%" stop-color="#ff6600" stop-opacity="0" />
				</radialGradient>
				<clipPath id="bai-route-clip">
					<rect x="496" y="228" width={routeW} height="52" />
				</clipPath>
			</defs>

			<!-- Sky + horizon glow -->
			<rect width="640" height="440" fill="url(#bai-sky)" />
			<rect width="640" height="440" fill="url(#bai-glow)" />

			<!-- Stars -->
			{#each STARS as s}
				<circle cx={s.x} cy={s.y} r={s.r} fill="white" opacity={reducedMotion ? 0.65 : starOp(s.p)} />
			{/each}

			<!-- Moon -->
			<g transform="translate(538, 78)" opacity={moonOp}>
				<circle r="58" fill="url(#bai-moonh)" />
				<circle r="22" fill="#fff5da" />
				<circle cx="6" cy="-5" r="5" fill="rgba(196,172,108,0.18)" />
				<circle cx="-8" cy="7" r="3.5" fill="rgba(196,172,108,0.13)" />
			</g>

			<!-- Water base + moon reflection -->
			<rect x="0" y="290" width="640" height="150" fill="url(#bai-water)" />
			<rect x="518" y="290" width="42" height="150" fill="url(#bai-refl)" opacity={moonOp} />

			<!-- Far waves (behind ship) -->
			<path d={W4} fill="#0d2248" class="wave4" class:paused={reducedMotion} />
			<path d={W3} fill="#112a56" class="wave3" class:paused={reducedMotion} />

			<!-- Wake trail -->
			<g transform="translate(0,{shipY})">
				<path
					d="M 148 306 Q 90 314 36 302 Q 12 298 2 294"
					stroke="rgba(255,255,255,0.13)"
					stroke-width="3.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 7"
				/>
				<path
					d="M 148 316 Q 80 328 28 318 Q 8 314 1 308"
					stroke="rgba(255,255,255,0.07)"
					stroke-width="2.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 10"
				/>
			</g>

			<!-- Route line -->
			<path
				d="M 496 274 Q 548 262 590 254 Q 620 248 636 244"
				stroke="rgba(255,255,255,0.42)"
				stroke-width="1.5"
				fill="none"
				stroke-dasharray="5 7"
				clip-path="url(#bai-route-clip)"
			/>
			{#if routeW > 2}
				<circle cx="496" cy="274" r="3" fill="rgba(255,255,255,0.36)" />
			{/if}

			<!-- ══ SHIP GROUP (translates + rotates with bob) ══════════════════ -->
			<g transform="translate(0,{shipY}) rotate({shipRot},308,285)">
				<!-- Bowsprit -->
				<line x1="488" y1="288" x2="540" y2="258" stroke="#1a2848" stroke-width="6" stroke-linecap="round" />
				<line
					x1="488"
					y1="288"
					x2="540"
					y2="258"
					stroke="#324e82"
					stroke-width="2.5"
					stroke-linecap="round"
				/>

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
				<line
					x1="254"
					y1="122"
					x2="362"
					y2="122"
					stroke="#1a2848"
					stroke-width="5.5"
					stroke-linecap="round"
				/>
				<line x1="254" y1="122" x2="362" y2="122" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
				<!-- Main topsail y=78 -->
				<line x1="270" y1="78" x2="346" y2="78" stroke="#1a2848" stroke-width="4.5" stroke-linecap="round" />
				<line x1="270" y1="78" x2="346" y2="78" stroke="#324e82" stroke-width="1.8" stroke-linecap="round" />
				<!-- Fore yard y=136 -->
				<line x1="361" y1="136" x2="469" y2="136" stroke="#1a2848" stroke-width="5" stroke-linecap="round" />
				<line x1="361" y1="136" x2="469" y2="136" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
				<!-- Mizzen yard y=116 -->
				<line
					x1="166"
					y1="116"
					x2="264"
					y2="116"
					stroke="#1a2848"
					stroke-width="4.5"
					stroke-linecap="round"
				/>
				<line
					x1="166"
					y1="116"
					x2="264"
					y2="116"
					stroke="#324e82"
					stroke-width="1.8"
					stroke-linecap="round"
				/>

				<!-- ── Sails (always curved — 10–14 px baseline bow-forward billow) ── -->

				<!-- Mizzen sail -->
				<path
					d="M 166 116 L 264 116
           Q {264 + 10 + mzR} 197 {264 + 6 + mzR * 0.5} 282
           L {166 - 4 + mzL} 282
           Q {166 - 4 - mzL * 0.6} 197 166 116 Z"
					fill="url(#bai-tsail)"
					opacity="0.88"
				/>

				<!-- Main lower sail — has emblem -->
				<path
					d="M 254 122 L 362 122
           Q {362 + 14 + mlR} 200 {362 + 8 + mlR * 0.5} 282
           L {254 - 5 + mlL} 282
           Q {254 - 5 - mlL * 0.6} 200 254 122 Z"
					fill="url(#bai-sail)"
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

				<!-- The crew's flag painted on the main sail: globe + straw hat (our mark, not a skull). Dark paint on
				     the light cloth so it reads at a glance; it rides the sail's billow with the rest of the layer. -->
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
					fill="url(#bai-tsail)"
					opacity="0.88"
				/>

				<!-- Fore lower sail -->
				<path
					d="M 361 136 L 469 136
           Q {469 + 12 + fsR} 208 {469 + 6 + fsR * 0.5} 284
           L {361 - 4 + fsL} 284
           Q {361 - 4 - fsL * 0.6} 208 361 136 Z"
					fill="url(#bai-sail)"
					opacity="0.90"
				/>

				<!-- Jib (triangular, bowsprit → fore mast) -->
				<path d="M 415 72 Q {477 + jibCtrl} 162 540 257 L 415 284 Z" fill="url(#bai-tsail)" opacity="0.72" />

				<!-- ── Rigging ── -->
				<line x1="308" y1="50" x2="540" y2="257" stroke="#324e82" stroke-width="1.2" opacity="0.55" />
				<line x1="308" y1="50" x2="200" y2="282" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
				<line x1="308" y1="50" x2="415" y2="282" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
				<!-- Ratlines (horizontal rungs across main shrouds) -->
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
				<rect
					x="291"
					y="79"
					width="34"
					height="17"
					rx="3"
					fill="#152038"
					stroke="#324e82"
					stroke-width="1.5"
				/>
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
				<circle cx="152" cy="258" r="18" fill="url(#bai-lantern)" opacity="0.7" />
				<circle cx="152" cy="258" r="5" fill="#ffa030" opacity="0.9" />
				<circle cx="164" cy="254" r="14" fill="url(#bai-lantern)" opacity="0.5" />
				<circle cx="164" cy="254" r="4" fill="#ffb840" opacity="0.85" />
			</g>
			<!-- end ship group -->

			<!-- Compass rose -->
			<g transform="translate(52,394)" opacity={compassOp}>
				<circle r="20" stroke="rgba(255,255,255,0.28)" stroke-width="1" fill="none" />
				{#each [0, 90, 180, 270] as ang}
					<line
						x1="0"
						y1="-20"
						x2="0"
						y2="-10"
						stroke="rgba(255,255,255,0.45)"
						stroke-width="1.5"
						transform="rotate({ang})"
					/>
				{/each}
				{#each [45, 135, 225, 315] as ang}
					<line
						x1="0"
						y1="-20"
						x2="0"
						y2="-14"
						stroke="rgba(255,255,255,0.28)"
						stroke-width="1"
						transform="rotate({ang})"
					/>
				{/each}
				<path d="M 0 -20 L -3 -12 L 0 -14 L 3 -12 Z" fill="rgba(255,255,255,0.48)" />
				<circle r="2.5" fill="rgba(255,255,255,0.38)" />
				<text y="-24" text-anchor="middle" fill="rgba(255,255,255,0.45)" font-size="7" font-family="inherit"
					>N</text
				>
			</g>

			<!-- Seagulls -->
			{#if gull1.vis}
				<g transform="translate({gull1.x},{gull1.y})">
					<path
						d="M -13 0 Q -6.5 -7.5 0 0"
						stroke="rgba(255,255,255,0.68)"
						stroke-width="2"
						stroke-linecap="round"
						fill="none"
					/>
					<path
						d="M 0 0 Q 6.5 -7.5 13 0"
						stroke="rgba(255,255,255,0.68)"
						stroke-width="2"
						stroke-linecap="round"
						fill="none"
					/>
				</g>
			{/if}
			{#if gull2.vis}
				<g transform="translate({gull2.x},{gull2.y})">
					<path
						d="M -10 0 Q -5 -5.5 0 0"
						stroke="rgba(255,255,255,0.52)"
						stroke-width="1.7"
						stroke-linecap="round"
						fill="none"
					/>
					<path
						d="M 0 0 Q 5 -5.5 10 0"
						stroke="rgba(255,255,255,0.52)"
						stroke-width="1.7"
						stroke-linecap="round"
						fill="none"
					/>
				</g>
			{/if}
		</svg>

		<!-- ═══ LAYER 2 — crew HTML (z=2): above sails, below hull rail ═══════════
         The crew-layer's z-index:2 in CSS places it above the bg SVG (z=1) and
         below the fg SVG hull (z=3), so faces clear the sail surface and the hull
         covers just the lower ~15 % of each face for a natural on-deck look.
		-->
		<div
			class="crew-layer"
			style:transform="translateY({shipY}px) rotate({shipRot}deg)"
			style:transform-origin="48.1% 64.8%"
		>
			<!--
        Crew positioning (feet behind the rail, so the whole face and hat clear it):
          hull rail y≈278; face-bottom target ≈ 278.  top% = 278/440 = 63.2%.  translateY(-100%).
          Stern-castle deck y=258; face-bottom 268; top% = 268/440 = 60.9%.
          Crow's nest: centre at y=86; top% = 86/440 = 19.5%. No sway of its own: the basket's front rim is drawn
          in the hull layer, which only moves with the ship, so a face swaying on its own floated out of it.
      -->

			<!-- A: crow's nest lookout — centre in the nest basket -->
			<div
				class="crew-face"
				style:left="48.1%"
				style:top="17.7%"
				style:transform="translateX(-50%) translateY(-50%)"
			>
				<BotFace seed="ship-crow" state="working" size={Math.round(42 * k)} />
			</div>

			<!-- B: bow lookout — scanning with working eyes -->
			<div
				class="crew-face"
				style:left="70.3%"
				style:top="63.2%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-a" state="working" size={Math.round(62 * k)} />
			</div>

			<!-- C: helmsman at stern castle steering wheel -->
			<div
				class="crew-face"
				style:left="29.7%"
				style:top="60.9%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-b" state={crew1State} size={Math.round(60 * k)} />
			</div>

			<!-- D: waving from midship rigging -->
			<div
				class="crew-face"
				style:left="55.5%"
				style:top="63.2%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-c" state={crew1State} size={Math.round(58 * k)} />
			</div>

			<!-- E: sleeping on a barrel (z's floating up) -->
			<div
				class="crew-face"
				style:left="41.3%"
				style:top="63.2%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-d" state="offline" size={Math.round(54 * k)} />
			</div>
		</div>

		<!-- ═══ LAYER 3 — fg SVG (z=3): hull occludes crew feet · near waves ═══════ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="3">
			<defs>
				<linearGradient id="bai-hull" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1c2c54" />
					<stop offset="100%" stop-color="#0c1730" />
				</linearGradient>
				<linearGradient id="bai-castle" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1e3062" />
					<stop offset="100%" stop-color="#162244" />
				</linearGradient>
			</defs>

			<!-- Hull + stern castle, bob with same transform as rigging -->
			<g transform="translate(0,{shipY}) rotate({shipRot},308,285)">
				<!-- Main hull body: deck y≈272, keel y≈355 → 83 px deep -->
				<path
					d="M 146 280
               Q 226 270 308 268
               Q 390 270 488 280
               L 496 292
               Q 454 325 308 330
               Q 162 325 146 294 Z"
					fill="url(#bai-hull)"
				/>

				<!-- Stern castle (raised poop deck, y 258–280) -->
				<path
					d="M 146 280
               L 146 258
               Q 180 254 228 254
               L 230 280
               Q 188 282 146 280 Z"
					fill="url(#bai-castle)"
				/>
				<line x1="228" y1="254" x2="230" y2="280" stroke="#2a3e68" stroke-width="1.5" fill="none" />

				<!-- Crow's-nest front rim in fg layer (occludes nest crew lower portion) -->
				<rect
					x="291"
					y="91"
					width="34"
					height="9"
					rx="2"
					fill="#152038"
					stroke="#324e82"
					stroke-width="1.2"
				/>

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
				<path
					d="M 146 280 Q 226 270 308 268 Q 390 270 488 280"
					stroke="#4a5e92"
					stroke-width="1.5"
					fill="none"
				/>

				<!-- 4 cannon ports -->
				<rect x="204" y="298" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
				<rect x="270" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
				<rect x="336" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
				<rect x="402" y="299" width="20" height="12" rx="2" fill="#060e1e" opacity="0.88" />
			</g>

			<!-- Wave band 2 -->
			<path d={W2} fill="#153366" class="wave2" class:paused={reducedMotion} />
			<path
				d={W2L}
				stroke="rgba(255,255,255,0.09)"
				stroke-width="1.5"
				fill="none"
				class="wave2"
				class:paused={reducedMotion}
			/>

			<!-- Wave band 1 (nearest) -->
			<path d={W1} fill="#1b3e7a" class="wave1" class:paused={reducedMotion} />
			<path
				d={W1L}
				stroke="rgba(255,255,255,0.18)"
				stroke-width="2"
				fill="none"
				class="wave1"
				class:paused={reducedMotion}
			/>
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

	/* Half the stage height (full width × 440/640 / 2). */
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

	/* Fixed-ratio stage: crew % positions align with SVG 640×440 viewBox; edge to edge across the column. */
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

	/* z-index: 2 places the crew ABOVE the bg SVG sails (z=1)
     and BELOW the fg SVG hull rail (z=3). */
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

	/* Wave CSS drift — linear is correct for continuous translation */
	@keyframes wave-scroll {
		from {
			transform: translateX(0);
		}
		to {
			transform: translateX(-640px);
		}
	}

	.wave4 {
		animation: wave-scroll 22s linear infinite;
	}
	.wave3 {
		animation: wave-scroll 13s linear infinite;
		animation-delay: -4s;
	}
	.wave2 {
		animation: wave-scroll 8s linear infinite;
		animation-delay: -2s;
	}
	.wave1 {
		animation: wave-scroll 4.8s linear infinite;
		animation-delay: -1.2s;
	}

	.paused {
		animation-play-state: paused !important;
	}

	@media (prefers-reduced-motion: reduce) {
		.wave4,
		.wave3,
		.wave2,
		.wave1 {
			animation-play-state: paused;
		}
	}
</style>
