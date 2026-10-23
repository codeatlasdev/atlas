<!--
  BotArtIntro — step 1 illustration.
  A pirate tall ship (three masts, square sails, crow's nest, stern castle) at dusk.
  4–5 BotFace crew clearly visible above the hull rail.
  Three z-layers: bg SVG (sky/sails/rigging) / crew HTML / fg SVG (hull + near waves).
  10 s scripted intro then calm idle loop.  Reduced-motion → still frame.
  Pauses on document.hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// ─── Wave geometry ──────────────────────────────────────────────────────────
	// Period 160 px × 16 half-periods = 1280 px (2 × viewBox).
	// CSS animation shifts −640 px → seamless tile.

	function waveD(y: number, amp: number): string {
		const pts: string[] = [`M 0 ${y}`];
		for (let i = 0; i < 16; i++) {
			const x0 = i * 80;
			const cy = i % 2 === 0 ? y - amp : y + amp;
			pts.push(`Q ${x0 + 40} ${cy} ${x0 + 80} ${y}`);
		}
		pts.push(`L 1280 440 L 0 440 Z`);
		return pts.join(" ");
	}

	function waveTop(y: number, amp: number): string {
		const pts: string[] = [`M 0 ${y}`];
		for (let i = 0; i < 16; i++) {
			const x0 = i * 80;
			const cy = i % 2 === 0 ? y - amp : y + amp;
			pts.push(`Q ${x0 + 40} ${cy} ${x0 + 80} ${y}`);
		}
		return pts.join(" ");
	}

	// Waves all below the hull waterline (y ≥ 295)
	const W4 = waveD(296, 4); // horizon shimmer
	const W3 = waveD(312, 8);
	const W2 = waveD(330, 12);
	const W1 = waveD(350, 17);
	const W1top = waveTop(350, 17);
	const W2top = waveTop(330, 12);

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

	// Deck bob centre (x=308, y=280) — slightly offset freqs for overlapping action
	const shipY = $derived(Math.sin(ph) * 7);
	const shipRot = $derived(Math.cos(ph * 1.07) * 2.2);

	// Crow's nest amplifies the mast-top sway (follow-through)
	const nestSway = $derived(Math.sin(ph * 1.07) * 5);

	// ─── Sail billow (0 = flat, 1 = full) ──────────────────────────────────────

	const sailBillow = $derived(
		t < 1
			? Math.max(0, spring(t, 0.05, 0.28))
			: 0.88 + Math.sin(ph * 0.58) * 0.07 + Math.sin(ph * 1.31) * 0.03,
	);

	// Per-sail derived control points (billow pushes leading / starboard edge forward)
	// Main lower sail: yard y=122, x 256→360; foot y=282
	const mlR = $derived(sailBillow * 30); // right edge bow-forward
	const mlL = $derived(sailBillow * 6); // left edge slight concave

	// Main topsail: yard y=78, x 272→344; foot y=122
	const mtR = $derived(sailBillow * 16);
	const mtL = $derived(sailBillow * 3);

	// Fore sail: yard y=136, x 363→467; foot y=283
	const fsR = $derived(sailBillow * 26);
	const fsL = $derived(sailBillow * 5);

	// Mizzen sail: yard y=116, x 168→262; foot y=282
	const mzR = $derived(sailBillow * 20);
	const mzL = $derived(sailBillow * 4);

	// Jib (triangular): fore-mast top → bowsprit tip → fore-mast foot
	const jibCtrl = $derived(sailBillow * 24);

	// ─── Pennant ────────────────────────────────────────────────────────────────

	const pennantAngle = $derived(-16 + Math.sin(ph * 2.9) * 10 + Math.sin(ph * 5.3) * 3.5);

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
		if (t < 1) {
			p = seg(t, iS, iE);
		} else {
			const cycle = ((ph * 0.19 + off) % (Math.PI * 2)) / (Math.PI * 2);
			p = cycle < 0.38 ? cycle / 0.38 : -1;
		}
		if (p < 0 || p > 1) return { x: -60, y: baseY, vis: false };
		return { x: 700 - p * 770, y: baseY + Math.sin(p * Math.PI * 2.2) * 14, vis: true };
	}

	const gull1 = $derived(gullPos(0.48, 0.68, 0, 80));
	const gull2 = $derived(gullPos(0.52, 0.72, 2.1, 65));

	// Crew individual follow-through
	const crew0extra = $derived(-Math.abs(Math.sin(ph * 0.94 + 0.35)) * 6);
	const crew1State = $derived(
		!reducedMotion && t > 0.18 && t < 0.6 ? ("needs_you" as const) : ("idle" as const),
	);

	// ─── Mount ──────────────────────────────────────────────────────────────────

	onMount(() => {
		if (reducedMotion) return;
		const introStart = performance.now();
		let lastNow = introStart;
		let raf: number;
		const tick = (now: number) => {
			if (!paused) {
				const dt = (now - lastNow) / 1000;
				ph += dt * Math.PI * 0.44;
				t = Math.min(1, (now - introStart) / INTRO_MS);
			}
			lastNow = now;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			paused = document.hidden;
			if (!paused) lastNow = performance.now();
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="ship-scene" aria-hidden="true">
	<div class="stage">
		<!-- ═══ LAYER 1 — background SVG: sky · water · rigging · sails ═══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="1">
			<defs>
				<linearGradient id="bai-sky" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#08102a" />
					<stop offset="40%" stop-color="#121e50" />
					<stop offset="75%" stop-color="#1a3272" />
					<stop offset="100%" stop-color="#1f3a7a" />
				</linearGradient>
				<radialGradient id="bai-horizon" cx="50%" cy="100%" r="68%">
					<stop offset="0%" stop-color="#d06218" stop-opacity="0.42" />
					<stop offset="55%" stop-color="#d06218" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#d06218" stop-opacity="0" />
				</radialGradient>
				<radialGradient id="bai-moon-h" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#fff8e4" stop-opacity="0.28" />
					<stop offset="55%" stop-color="#ffc860" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#ffc860" stop-opacity="0" />
				</radialGradient>
				<linearGradient id="bai-water" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0b1e42" />
					<stop offset="100%" stop-color="#060e20" />
				</linearGradient>
				<!-- Sail: horizontal gradient, brighter centre = light catching billow -->
				<linearGradient id="bai-sail" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#b6c8e8" />
					<stop offset="40%" stop-color="#ddeafc" />
					<stop offset="100%" stop-color="#c8d8f0" />
				</linearGradient>
				<linearGradient id="bai-tsail" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#b0c2e0" />
					<stop offset="50%" stop-color="#d4e4f8" />
					<stop offset="100%" stop-color="#c2d2ec" />
				</linearGradient>
				<linearGradient id="bai-refl" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#ffc860" stop-opacity="0.22" />
					<stop offset="100%" stop-color="#ffc860" stop-opacity="0" />
				</linearGradient>
				<!-- Lantern glow -->
				<radialGradient id="bai-lantern" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#ffb030" stop-opacity="0.85" />
					<stop offset="60%" stop-color="#ff8820" stop-opacity="0.30" />
					<stop offset="100%" stop-color="#ff6600" stop-opacity="0" />
				</radialGradient>
				<clipPath id="bai-route-clip">
					<rect x="492" y="228" width={routeW} height="52" />
				</clipPath>
			</defs>

			<!-- ─── Sky ──────────────────────────────────────────────────── -->
			<rect width="640" height="440" fill="url(#bai-sky)" />
			<rect width="640" height="440" fill="url(#bai-horizon)" />

			<!-- ─── Stars ────────────────────────────────────────────────── -->
			{#each STARS as s}
				<circle cx={s.x} cy={s.y} r={s.r} fill="white" opacity={reducedMotion ? 0.65 : starOp(s.p)} />
			{/each}

			<!-- ─── Moon ──────────────────────────────────────────────────── -->
			<g transform="translate(538, 78)" opacity={moonOp}>
				<circle r="58" fill="url(#bai-moon-h)" />
				<circle r="22" fill="#fff5da" />
				<circle cx="6" cy="-5" r="5" fill="rgba(196,172,108,0.18)" />
				<circle cx="-8" cy="7" r="3.5" fill="rgba(196,172,108,0.13)" />
			</g>

			<!-- ─── Water base ────────────────────────────────────────────── -->
			<rect x="0" y="288" width="640" height="152" fill="url(#bai-water)" />
			<rect x="518" y="288" width="42" height="152" fill="url(#bai-refl)" opacity={moonOp} />

			<!-- ─── Far waves (behind ship) ───────────────────────────────── -->
			<path d={W4} fill="#0d2348" class="wave4" class:paused={reducedMotion} />
			<path d={W3} fill="#112b56" class="wave3" class:paused={reducedMotion} />

			<!-- ─── Wake trail ─────────────────────────────────────────────── -->
			<g transform="translate(0,{shipY})">
				<path
					d="M 148 300 Q 90 308 38 296 Q 14 292 4 288"
					stroke="rgba(255,255,255,0.14)"
					stroke-width="3.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 7"
				/>
				<path
					d="M 148 310 Q 82 322 30 312 Q 10 308 2 302"
					stroke="rgba(255,255,255,0.08)"
					stroke-width="2.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 10"
				/>
			</g>

			<!-- ─── Route line ahead of bow ───────────────────────────────── -->
			<path
				d="M 496 268 Q 548 256 590 248 Q 620 242 636 238"
				stroke="rgba(255,255,255,0.42)"
				stroke-width="1.5"
				fill="none"
				stroke-dasharray="5 7"
				clip-path="url(#bai-route-clip)"
			/>
			{#if routeW > 2}
				<circle cx="496" cy="268" r="3" fill="rgba(255,255,255,0.36)" />
			{/if}

			<!-- ═══ SHIP GROUP (bobs with hull) ═══════════════════════════════ -->
			<g transform="translate(0,{shipY}) rotate({shipRot},308,280)">
				<!-- ─── Bowsprit (forward spar, angled up-right) ──────────── -->
				<line x1="490" y1="285" x2="540" y2="256" stroke="#1a2848" stroke-width="6" stroke-linecap="round" />
				<line
					x1="490"
					y1="285"
					x2="540"
					y2="256"
					stroke="#324e82"
					stroke-width="2.5"
					stroke-linecap="round"
				/>

				<!-- ─── Fore mast (x=415) ────────────────────────────────── -->
				<line x1="415" y1="282" x2="415" y2="72" stroke="#1a2848" stroke-width="6" stroke-linecap="round" />
				<line x1="415" y1="282" x2="415" y2="72" stroke="#324e82" stroke-width="2.5" stroke-linecap="round" />

				<!-- ─── Main mast (x=308) ────────────────────────────────── -->
				<line x1="308" y1="280" x2="308" y2="52" stroke="#1a2848" stroke-width="7" stroke-linecap="round" />
				<line x1="308" y1="280" x2="308" y2="52" stroke="#324e82" stroke-width="3" stroke-linecap="round" />

				<!-- ─── Mizzen mast (x=215) ──────────────────────────────── -->
				<line x1="215" y1="280" x2="215" y2="84" stroke="#1a2848" stroke-width="5.5" stroke-linecap="round" />
				<line x1="215" y1="280" x2="215" y2="84" stroke="#324e82" stroke-width="2" stroke-linecap="round" />

				<!-- ─── Yards (cross-spars) ──────────────────────────────── -->
				<!-- Main lower yard (y=122) -->
				<line
					x1="256"
					y1="122"
					x2="360"
					y2="122"
					stroke="#1a2848"
					stroke-width="5.5"
					stroke-linecap="round"
				/>
				<line x1="256" y1="122" x2="360" y2="122" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
				<!-- Main topsail yard (y=78) -->
				<line x1="272" y1="78" x2="344" y2="78" stroke="#1a2848" stroke-width="4.5" stroke-linecap="round" />
				<line x1="272" y1="78" x2="344" y2="78" stroke="#324e82" stroke-width="1.8" stroke-linecap="round" />
				<!-- Fore yard (y=136) -->
				<line x1="363" y1="136" x2="467" y2="136" stroke="#1a2848" stroke-width="5" stroke-linecap="round" />
				<line x1="363" y1="136" x2="467" y2="136" stroke="#324e82" stroke-width="2" stroke-linecap="round" />
				<!-- Mizzen yard (y=116) -->
				<line
					x1="168"
					y1="116"
					x2="262"
					y2="116"
					stroke="#1a2848"
					stroke-width="4.5"
					stroke-linecap="round"
				/>
				<line
					x1="168"
					y1="116"
					x2="262"
					y2="116"
					stroke="#324e82"
					stroke-width="1.8"
					stroke-linecap="round"
				/>

				<!-- ─── Sails ─────────────────────────────────────────────── -->
				<!-- Mizzen sail (stern-most) -->
				<path
					d="M 168 116 L 262 116
             Q {262 + mzR} 196 {262 + mzR * 0.5} 282
             L {168 + mzL} 282
             Q {168 - mzL * 0.6} 196 168 116 Z"
					fill="url(#bai-tsail)"
					opacity="0.85"
				/>

				<!-- Main lower sail (biggest — has our emblem) -->
				<path
					d="M 256 122 L 360 122
             Q {360 + mlR} 200 {360 + mlR * 0.5} 282
             L {256 + mlL} 282
             Q {256 - mlL * 0.6} 200 256 122 Z"
					fill="url(#bai-sail)"
					opacity="0.93"
				/>
				<!-- Batten lines for texture -->
				{#each [148, 174, 200, 228, 255] as by}
					<line
						x1={256 + (by - 122) * 0.06}
						y1={by}
						x2={360 + (by - 122) * 0.14}
						y2={by}
						stroke="rgba(160,188,228,0.20)"
						stroke-width="0.9"
					/>
				{/each}
				<!-- Globe + straw-hat emblem on main sail (our mark, not a skull) -->
				<g transform="translate(308,200)" opacity="0.28">
					<circle r="13" stroke="#c4d8f0" stroke-width="1.3" fill="none" />
					<path d="M -13 0 Q -6 -2 0 -2 Q 6 -2 13 0" stroke="#c4d8f0" stroke-width="0.9" fill="none" />
					<path d="M 0 -13 Q -3.5 0 -3.5 3 Q -3.5 6 0 13" stroke="#c4d8f0" stroke-width="0.8" fill="none" />
					<ellipse cx="0" cy="-16" rx="10" ry="2.2" stroke="#c4d8f0" stroke-width="1.2" fill="none" />
					<path d="M -7 -16 C -7 -22 7 -22 7 -16" stroke="#c4d8f0" stroke-width="1.2" fill="none" />
				</g>

				<!-- Main topsail -->
				<path
					d="M 272 78 L 344 78
             Q {344 + mtR} 100 {344 + mtR * 0.5} 122
             L {272 + mtL} 122
             Q {272 - mtL * 0.5} 100 272 78 Z"
					fill="url(#bai-tsail)"
					opacity="0.88"
				/>

				<!-- Fore lower sail -->
				<path
					d="M 363 136 L 467 136
             Q {467 + fsR} 208 {467 + fsR * 0.5} 283
             L {363 + fsL} 283
             Q {363 - fsL * 0.6} 208 363 136 Z"
					fill="url(#bai-sail)"
					opacity="0.90"
				/>

				<!-- Jib (triangular stay-sail, bowsprit to fore mast) -->
				<path
					d="M 415 72
             Q {476 + jibCtrl} {163} 540 257
             L 415 283 Z"
					fill="url(#bai-tsail)"
					opacity="0.72"
				/>

				<!-- ─── Rigging ────────────────────────────────────────────── -->
				<!-- Main mast: forestay to bowsprit tip -->
				<line x1="308" y1="52" x2="540" y2="256" stroke="#324e82" stroke-width="1.2" opacity="0.55" />
				<!-- Main mast: port shroud to hull left -->
				<line x1="308" y1="52" x2="202" y2="280" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
				<!-- Main mast: starboard shroud to hull right -->
				<line x1="308" y1="52" x2="415" y2="280" stroke="#324e82" stroke-width="1.2" opacity="0.52" />
				<!-- Ratlines (horizontal rungs across main shrouds): 4 intervals -->
				<!-- At y=125: port x≈277, stbd x≈338 -->
				<line x1="277" y1="125" x2="338" y2="125" stroke="#324e82" stroke-width="0.9" opacity="0.40" />
				<!-- At y=165 -->
				<line x1="258" y1="165" x2="358" y2="165" stroke="#324e82" stroke-width="0.9" opacity="0.38" />
				<!-- At y=205 -->
				<line x1="240" y1="205" x2="376" y2="205" stroke="#324e82" stroke-width="0.9" opacity="0.35" />
				<!-- At y=245 -->
				<line x1="222" y1="245" x2="396" y2="245" stroke="#324e82" stroke-width="0.9" opacity="0.32" />
				<!-- Fore mast: stay to bowsprit + shrouds -->
				<line x1="415" y1="72" x2="540" y2="256" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
				<line x1="415" y1="72" x2="308" y2="62" stroke="#324e82" stroke-width="0.9" opacity="0.40" />
				<line x1="415" y1="72" x2="490" y2="283" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
				<!-- Mizzen mast: shrouds -->
				<line x1="215" y1="84" x2="148" y2="278" stroke="#324e82" stroke-width="1.0" opacity="0.45" />
				<line x1="215" y1="84" x2="308" y2="280" stroke="#324e82" stroke-width="1.0" opacity="0.45" />

				<!-- ─── Crow's nest (main mast top, y≈82–96) ──────────────── -->
				<rect
					x="292"
					y="80"
					width="32"
					height="16"
					rx="3"
					fill="#152038"
					stroke="#324e82"
					stroke-width="1.5"
				/>
				<!-- Platform floor line -->
				<line x1="290" y1="96" x2="326" y2="96" stroke="#324e82" stroke-width="2" stroke-linecap="round" />

				<!-- ─── Stern castle details ───────────────────────────────── -->
				<!-- Stern castle windows (porthole row) -->
				<circle cx="158" cy="268" r="5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.2" />
				<circle cx="172" cy="268" r="5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.2" />
				<circle cx="186" cy="268" r="5" fill="#0a1428" stroke="#2a3e6a" stroke-width="1.2" />
				<!-- Warm light in middle porthole -->
				<circle cx="172" cy="268" r="3.5" fill="#ffa030" opacity="0.55" />

				<!-- Steering wheel at stern castle deck -->
				<g transform="translate(190,262)">
					<circle r="11" stroke="#3e5882" stroke-width="2.2" fill="none" />
					<line x1="-11" y1="0" x2="11" y2="0" stroke="#3e5882" stroke-width="1.4" />
					<line x1="0" y1="-11" x2="0" y2="11" stroke="#3e5882" stroke-width="1.4" />
					<line x1="-7.8" y1="-7.8" x2="7.8" y2="7.8" stroke="#3e5882" stroke-width="1.4" />
					<line x1="7.8" y1="-7.8" x2="-7.8" y2="7.8" stroke="#3e5882" stroke-width="1.4" />
					<circle r="3.5" fill="#3e5882" />
					<circle cx="0" cy="-11" r="2.2" fill="#3e5882" />
					<circle cx="7.8" cy="-7.8" r="2.2" fill="#3e5882" />
					<circle cx="11" cy="0" r="2.2" fill="#3e5882" />
					<circle cx="7.8" cy="7.8" r="2.2" fill="#3e5882" />
					<circle cx="0" cy="11" r="2.2" fill="#3e5882" />
					<circle cx="-7.8" cy="7.8" r="2.2" fill="#3e5882" />
					<circle cx="-11" cy="0" r="2.2" fill="#3e5882" />
					<circle cx="-7.8" cy="-7.8" r="2.2" fill="#3e5882" />
				</g>

				<!-- Stern lanterns (amber glow halos) -->
				<circle cx="153" cy="260" r="18" fill="url(#bai-lantern)" opacity="0.7" />
				<circle cx="153" cy="260" r="5" fill="#ffa030" opacity="0.9" />
				<circle cx="165" cy="256" r="14" fill="url(#bai-lantern)" opacity="0.5" />
				<circle cx="165" cy="256" r="4" fill="#ffb840" opacity="0.85" />
			</g>
			<!-- end ship group -->

			<!-- ─── Compass rose ──────────────────────────────────────────── -->
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

			<!-- ─── Seagulls ───────────────────────────────────────────────── -->
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

		<!-- ═══ LAYER 2 — crew HTML (z=2): BotFaces + pennant ═══ -->
		<!--
      Bob + roll matching the hull in layer 3.
      Transform-origin aligns with ship's rotation centre (308, 280).
      Percentages: 308/640 = 48.1%, 280/440 = 63.6%
		-->
		<div
			class="crew-layer"
			style:transform="translateY({shipY}px) rotate({shipRot}deg)"
			style:transform-origin="48.1% 63.6%"
		>
			<!-- ShipFlag pennant — our mark (globe + straw hat), NOT a skull -->
			<div
				class="pennant"
				style:left="48.1%"
				style:top="11.8%"
				style:color="rgba(255,255,255,0.82)"
				style="--color-accent:#ffd070"
				style:transform="translateX(2px) translateY(-100%) rotate({pennantAngle}deg)"
			>
				<ShipFlag size={24} />
			</div>

			<!--
        Crew: all positioned so face-bottom sits ~10 px below hull-deck (y=282),
        making hull fg-layer cover just the lower ~15% of each face.
        Deck line → y=282, face-bottom target → y=292.
        top = 292/440 × 100% = 66.4%.  transform: translateX(-50%) translateY(-100%).
        Stern-castle crew at y=272 → top = 61.8%.
      -->

			<!-- Crew A: crow's-nest lookout on main mast.
           Crow's nest y=80–96; face centre at y=82 → top=18.6%.
           Nest front-wall in fg-layer partially occludes lower portion. -->
			<div
				class="crew-face"
				style:left="48.1%"
				style:top="18.6%"
				style:transform="translateX(-50%) translateY(-50%) translateY({nestSway}px)"
			>
				<BotFace seed="ship-crow" state="working" size={40} />
			</div>

			<!-- Crew B: bow lookout — leans forward scanning the horizon -->
			<div
				class="crew-face"
				style:left="70.3%"
				style:top="66.4%"
				style:transform="translateX(-50%) translateY(-100%) translateY({crew0extra}px)"
			>
				<BotFace seed="ship-a" state="working" size={62} />
			</div>

			<!-- Crew C: helmsman at stern castle (steering wheel) — excited during sails beat -->
			<div
				class="crew-face"
				style:left="29.7%"
				style:top="61.8%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-b" state={crew1State} size={60} />
			</div>

			<!-- Crew D: waving from midship rigging -->
			<div
				class="crew-face"
				style:left="55.3%"
				style:top="66.4%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-c" state={crew1State} size={58} />
			</div>

			<!-- Crew E: sleeping on a barrel near main mast -->
			<div
				class="crew-face"
				style:left="41.3%"
				style:top="66.4%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="ship-d" state="offline" size={54} />
			</div>
		</div>

		<!-- ═══ LAYER 3 — foreground SVG: hull occludes crew feet · near waves ═══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="3">
			<defs>
				<linearGradient id="bai-hull" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1c2c54" />
					<stop offset="100%" stop-color="#0d1834" />
				</linearGradient>
				<linearGradient id="bai-castle" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1e3060" />
					<stop offset="100%" stop-color="#162244" />
				</linearGradient>
			</defs>

			<!-- ─── Hull + stern castle (bobs with ship) ──────────────────── -->
			<g transform="translate(0,{shipY}) rotate({shipRot},308,280)">
				<!-- Main hull body -->
				<path
					d="M 148 278
               Q 228 272 308 270
               Q 388 272 488 278
               L 494 288
               Q 454 308 308 312
               Q 162 308 148 290 Z"
					fill="url(#bai-hull)"
				/>

				<!-- Stern castle (raised poop deck above main deck) -->
				<path
					d="M 148 278
               L 148 258
               Q 180 254 225 254
               L 228 278 Q 188 280 148 278 Z"
					fill="url(#bai-castle)"
				/>
				<!-- Stern castle wall (darker edge) -->
				<path d="M 225 254 L 228 278" stroke="#2a3e68" stroke-width="1.5" fill="none" />

				<!-- Crow's-nest front rim (occludes lower portion of crow's-nest crew) -->
				<rect
					x="292"
					y="90"
					width="32"
					height="9"
					rx="2"
					fill="#152038"
					stroke="#324e82"
					stroke-width="1.2"
				/>

				<!-- Atlas-blue accent stripe (waterline) -->
				<path
					d="M 152 286 Q 234 282 308 280 Q 382 282 484 286"
					stroke="#3d5bff"
					stroke-width="2.5"
					fill="none"
					opacity="0.75"
				/>

				<!-- Deck surface (slightly lighter top face, shows sheer) -->
				<path
					d="M 148 278
               Q 228 272 308 270
               Q 388 272 488 278
               L 490 284
               Q 388 280 308 278
               Q 228 280 150 284 Z"
					fill="#1e2f5a"
				/>

				<!-- Deck rail cap line -->
				<path
					d="M 148 278 Q 228 272 308 270 Q 388 272 488 278"
					stroke="#4a5e90"
					stroke-width="1.5"
					fill="none"
				/>

				<!-- Cannon ports — row of 4 along hull -->
				<rect x="202" y="293" width="18" height="11" rx="2" fill="#060f20" opacity="0.85" />
				<rect x="266" y="294" width="18" height="11" rx="2" fill="#060f20" opacity="0.85" />
				<rect x="330" y="294" width="18" height="11" rx="2" fill="#060f20" opacity="0.85" />
				<rect x="394" y="294" width="18" height="11" rx="2" fill="#060f20" opacity="0.85" />
				<!-- Port lids (slightly open) -->
				<line x1="202" y1="293" x2="220" y2="293" stroke="#162040" stroke-width="1.5" />
				<line x1="266" y1="294" x2="284" y2="294" stroke="#162040" stroke-width="1.5" />
				<line x1="330" y1="294" x2="348" y2="294" stroke="#162040" stroke-width="1.5" />
				<line x1="394" y1="294" x2="412" y2="294" stroke="#162040" stroke-width="1.5" />
			</g>

			<!-- ─── Wave band 2 ────────────────────────────────────────────── -->
			<path d={W2} fill="#163468" class="wave2" class:paused={reducedMotion} />
			<path
				d={W2top}
				stroke="rgba(255,255,255,0.09)"
				stroke-width="1.5"
				fill="none"
				class="wave2"
				class:paused={reducedMotion}
			/>

			<!-- ─── Wave band 1 (nearest) ─────────────────────────────────── -->
			<path d={W1} fill="#1c3f7c" class="wave1" class:paused={reducedMotion} />
			<path
				d={W1top}
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
	/* ─── Outer shell ──────────────────────────────────────────────── */
	.ship-scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}

	/* Fixed-ratio stage: crew %-positions align with the SVG viewBox */
	.stage {
		position: relative;
		width: min(96%, 640px);
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

	/* ─── Crew HTML layer ──────────────────────────────────────────── */
	.crew-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
		will-change: transform;
	}

	.crew-face {
		position: absolute;
		/* x/y and transform set per-face via inline style */
	}

	.pennant {
		position: absolute;
		transform-origin: 0 100%;
		will-change: transform;
	}

	/* ─── Wave animations (continuous drift — linear is correct) ───── */
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
