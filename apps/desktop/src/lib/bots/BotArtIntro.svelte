<!--
  BotArtIntro — step 1 illustration.
  A small elegant sailing ship at dusk: 10 s scripted intro then a calm idle loop.
  Three HTML layers (bg SVG / crew HTML / fg SVG) let BotFace components peek over
  the hull rail while the foreground waves give depth.
  Reduced-motion: a single beautiful still frame. Pauses when document is hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// ─── Wave geometry ─────────────────────────────────────────────────────────
	// Period 160 px, 16 half-periods → 1280 px total (2× viewBox width).
	// CSS animation shifts -640 px → seamless loop.

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

	const W4 = waveD(268, 5);
	const W3 = waveD(282, 9);
	const W2 = waveD(300, 13);
	const W1 = waveD(318, 18);
	const W1top = waveTop(318, 18);
	const W2top = waveTop(300, 13);

	// ─── Animation state ───────────────────────────────────────────────────────

	const INTRO_MS = 10_000;
	// t: 0 → 1 over the 10 s intro, then stays 1 (idle)
	let t = $state(reducedMotion ? 1 : 0);
	// ph: continuous radians, drives bob / flutter / twinkle
	let ph = $state(reducedMotion ? 1.1 : 0);
	let paused = $state(false);

	// ─── Easing ────────────────────────────────────────────────────────────────

	// Standard ease-in-out (smoothstep)
	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Spring: anticipation dip then overshoot past 1
	function spring(x: number, a: number, b: number): number {
		const raw = (x - a) / (b - a);
		if (raw <= 0) return 0;
		if (raw >= 1.2) return 1;
		// Ease through [0,1] with slight overshoot at end
		const p = Math.min(1, raw);
		const base = p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
		if (raw <= 1) return base;
		const over = (raw - 1) / 0.2;
		return 1 + Math.sin(over * Math.PI) * 0.11;
	}

	// ─── Derived motion ────────────────────────────────────────────────────────

	// Ship bob + roll (offset frequencies → overlapping action)
	const shipY = $derived(Math.sin(ph) * 7);
	const shipRot = $derived(Math.cos(ph * 1.07) * 2.3);

	// Sail billow: 0 = flat, 1 = full
	// Beat t=0.06→0.30 with spring overshoot; idle: gentle secondary flutter
	const sailBillow = $derived(
		t < 1
			? Math.max(0, spring(t, 0.05, 0.28))
			: 0.88 + Math.sin(ph * 0.58) * 0.07 + Math.sin(ph * 1.3) * 0.03,
	);

	// Main sail left-edge control point (bows to the right = toward bow, wind from stern)
	const sailCtrlX = $derived(232 + sailBillow * 58);
	// Fore sail (jib) leading-edge bulge
	const jibCtrlX = $derived(409 + sailBillow * 32);

	// Pennant flutter (rotates around pole attachment = left edge)
	const pennantAngle = $derived(-18 + Math.sin(ph * 2.9) * 10 + Math.sin(ph * 5.3) * 3.5);

	// Route line clip width: grows from 0 → 190 during t=0.42→0.80
	const routeW = $derived(t < 1 ? seg(t, 0.42, 0.8) * 190 : 190);

	// Compass rose opacity
	const compassOp = $derived(t < 1 ? seg(t, 0.65, 0.92) * 0.28 : 0.28);

	// Moon fade-in at scene open
	const moonOp = $derived(t < 1 ? seg(t, 0, 0.22) : 1);

	// Stars (8, different phase offsets → no lockstep twinkle)
	const STARS = [
		{ x: 68, y: 44, r: 1.4, p: 0.0 },
		{ x: 142, y: 26, r: 1.1, p: 1.3 },
		{ x: 228, y: 56, r: 1.5, p: 2.5 },
		{ x: 308, y: 16, r: 1.2, p: 0.7 },
		{ x: 388, y: 38, r: 1.3, p: 3.1 },
		{ x: 462, y: 20, r: 1.0, p: 1.9 },
		{ x: 522, y: 52, r: 1.4, p: 4.3 },
		{ x: 582, y: 30, r: 1.1, p: 2.8 },
	] as const;

	function starOp(phase: number): number {
		return 0.28 + 0.72 * (0.5 + 0.5 * Math.sin(ph * 1.8 + phase));
	}

	// Seagulls: two M-shaped birds that cross occasionally
	// Returns { x, y, vis } in SVG coordinates
	function gullPos(
		introStart: number,
		introEnd: number,
		idleOff: number,
		baseY: number,
	): { x: number; y: number; vis: boolean } {
		let p = -1;
		if (t < 1) {
			p = seg(t, introStart, introEnd);
		} else {
			const cycle = ((ph * 0.19 + idleOff) % (Math.PI * 2)) / (Math.PI * 2);
			p = cycle < 0.38 ? cycle / 0.38 : -1;
		}
		if (p < 0 || p > 1) return { x: -60, y: baseY, vis: false };
		// Fly right-to-left with gentle altitude sine
		return { x: 690 - p * 760, y: baseY + Math.sin(p * Math.PI * 2.2) * 14, vis: true };
	}

	const gull1 = $derived(gullPos(0.48, 0.68, 0, 86));
	const gull2 = $derived(gullPos(0.52, 0.72, 2.1, 70));

	// Crew individual actions (follow-through on top of ship bob)
	// Crew 0 (front, hopper): extra vertical follow-through, slightly offset phase
	const crew0extra = $derived(-Math.abs(Math.sin(ph * 0.94 + 0.35)) * 6);
	// Crew 1 (waver): excited face during the "sails fill" beat
	const crew1State = $derived(
		!reducedMotion && t > 0.18 && t < 0.6 ? ("needs_you" as const) : ("idle" as const),
	);

	// ─── Mount ─────────────────────────────────────────────────────────────────

	onMount(() => {
		if (reducedMotion) return;

		const introStart = performance.now();
		let lastNow = introStart;
		let raf: number;

		const tick = (now: number) => {
			if (!paused) {
				const dt = (now - lastNow) / 1000;
				// Bob period ≈ 4.5 s (π × 0.44 rad/s)
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
		<!-- ═══ LAYER 1: background SVG (sky · water · sails · rigging · scene FX) ═══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="1">
			<defs>
				<linearGradient id="bai-sky" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0b1130" />
					<stop offset="42%" stop-color="#142055" />
					<stop offset="78%" stop-color="#1a3470" />
					<stop offset="100%" stop-color="#1f3c78" />
				</linearGradient>
				<radialGradient id="bai-horizon" cx="50%" cy="100%" r="65%" fx="50%" fy="100%">
					<stop offset="0%" stop-color="#cf6520" stop-opacity="0.40" />
					<stop offset="55%" stop-color="#cf6520" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#cf6520" stop-opacity="0" />
				</radialGradient>
				<radialGradient id="bai-moon-halo" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#fff8e6" stop-opacity="0.28" />
					<stop offset="55%" stop-color="#ffd070" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#ffd070" stop-opacity="0" />
				</radialGradient>
				<linearGradient id="bai-water" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0b1e42" />
					<stop offset="100%" stop-color="#060f22" />
				</linearGradient>
				<linearGradient id="bai-sail" x1="0" y1="0" x2="1" y2="0">
					<stop offset="0%" stop-color="#ccd8f0" />
					<stop offset="100%" stop-color="#e2ecfc" />
				</linearGradient>
				<linearGradient id="bai-refl" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#ffd070" stop-opacity="0.22" />
					<stop offset="100%" stop-color="#ffd070" stop-opacity="0" />
				</linearGradient>
				<!-- Route-line clip: grows from bow forward -->
				<clipPath id="bai-route-clip">
					<rect x="422" y="218" width={routeW} height="58" />
				</clipPath>
			</defs>

			<!-- ─── Sky ────────────────────────────────────────────────── -->
			<rect x="0" y="0" width="640" height="440" fill="url(#bai-sky)" />
			<rect x="0" y="0" width="640" height="440" fill="url(#bai-horizon)" />

			<!-- ─── Stars ─────────────────────────────────────────────── -->
			{#each STARS as s}
				<circle cx={s.x} cy={s.y} r={s.r} fill="white" opacity={reducedMotion ? 0.68 : starOp(s.p)} />
			{/each}

			<!-- ─── Moon ──────────────────────────────────────────────── -->
			<g transform="translate(536, 80)" opacity={moonOp}>
				<circle cx="0" cy="0" r="58" fill="url(#bai-moon-halo)" />
				<circle cx="0" cy="0" r="23" fill="#fff5dc" />
				<circle cx="7" cy="-6" r="5.5" fill="rgba(195,172,110,0.18)" />
				<circle cx="-9" cy="8" r="3.5" fill="rgba(195,172,110,0.13)" />
			</g>

			<!-- ─── Water base ────────────────────────────────────────── -->
			<rect x="0" y="260" width="640" height="180" fill="url(#bai-water)" />
			<!-- Moon reflection strip -->
			<rect x="516" y="260" width="44" height="180" fill="url(#bai-refl)" opacity={moonOp} />

			<!-- ─── Wave band 4 (horizon shimmer) ────────────────────── -->
			<path d={W4} fill="#0e2550" class="wave4" class:paused={reducedMotion} />

			<!-- ─── Wave band 3 ───────────────────────────────────────── -->
			<path d={W3} fill="#122d5e" class="wave3" class:paused={reducedMotion} />

			<!-- ─── Wake trail (behind stern) ────────────────────────── -->
			<g transform="translate(0, {shipY})">
				<path
					d="M 192 290 Q 125 298 58 288 Q 28 284 8 282"
					stroke="rgba(255,255,255,0.16)"
					stroke-width="3.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 6"
				/>
				<path
					d="M 192 300 Q 118 309 46 300 Q 22 296 6 292"
					stroke="rgba(255,255,255,0.09)"
					stroke-width="2.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 9"
				/>
			</g>

			<!-- ─── Route line (draws ahead of bow) ──────────────────── -->
			<path
				d="M 430 260 Q 488 246 535 240 Q 575 234 612 230"
				stroke="rgba(255,255,255,0.42)"
				stroke-width="1.5"
				fill="none"
				stroke-dasharray="5 7"
				clip-path="url(#bai-route-clip)"
			/>
			<!-- Bow marker dot -->
			{#if routeW > 2}
				<circle cx="430" cy="260" r="3" fill="rgba(255,255,255,0.38)" />
			{/if}

			<!-- ─── Ship rigging + sails (bobs with hull) ─────────────── -->
			<g transform="translate(0, {shipY}) rotate({shipRot}, 295, 260)">
				<!-- Bowsprit -->
				<line x1="418" y1="266" x2="468" y2="244" stroke="#1e2e55" stroke-width="5" stroke-linecap="round" />
				<line x1="418" y1="266" x2="468" y2="244" stroke="#364f84" stroke-width="2" stroke-linecap="round" />

				<!-- Stays (forestay + backstay wire lines) -->
				<line x1="468" y1="244" x2="352" y2="106" stroke="#364f84" stroke-width="1.2" opacity="0.55" />
				<line x1="352" y1="106" x2="283" y2="88" stroke="#364f84" stroke-width="1" opacity="0.45" />
				<line x1="283" y1="75" x2="468" y2="244" stroke="#364f84" stroke-width="0.9" opacity="0.35" />

				<!-- Main mast (x=283) -->
				<line x1="283" y1="256" x2="283" y2="75" stroke="#1e2e55" stroke-width="6" stroke-linecap="round" />
				<line x1="283" y1="256" x2="283" y2="75" stroke="#364f84" stroke-width="2.5" stroke-linecap="round" />

				<!-- Fore mast (x=352) -->
				<line x1="352" y1="258" x2="352" y2="106" stroke="#1e2e55" stroke-width="5" stroke-linecap="round" />
				<line x1="352" y1="258" x2="352" y2="106" stroke="#364f84" stroke-width="2" stroke-linecap="round" />

				<!-- Main cross-yard (x 232→334 at y=110) -->
				<line x1="232" y1="110" x2="334" y2="110" stroke="#1e2e55" stroke-width="5" stroke-linecap="round" />
				<line x1="232" y1="110" x2="334" y2="110" stroke="#364f84" stroke-width="2" stroke-linecap="round" />

				<!-- Main sail (bows toward bow / right as wind fills from stern) -->
				<!-- Batten lines for subtle texture (4 horizontal) -->
				{#each [140, 168, 196, 224] as sy}
					<line
						x1={232 + (sailBillow * 0.35 * (sy - 110)) / 143}
						y1={sy}
						x2="334"
						y2={sy}
						stroke="rgba(165,190,235,0.22)"
						stroke-width="0.9"
					/>
				{/each}
				<path
					d="M 334 110
             L 232 110
             Q {sailCtrlX} 182 232 253
             L {283 + sailBillow * 10} 253
             Q {314 + sailBillow * 14} 182 334 110
             Z"
					fill="url(#bai-sail)"
					opacity="0.93"
				/>

				<!-- Jib sail (fore-mast top → bowsprit tip → fore-mast foot) -->
				<path
					d="M 352 106
             Q {jibCtrlX} 177 468 244
             L 352 258
             Z"
					fill="url(#bai-sail)"
					opacity="0.78"
				/>

				<!-- Small cabin / wheelhouse near stern -->
				<rect x="196" y="244" width="42" height="16" rx="3" fill="#162040" />
				<rect x="198" y="246" width="38" height="12" rx="2" fill="#1e2e55" />
				<rect x="200" y="248" width="10" height="7" rx="1.5" fill="#3d5bff" opacity="0.45" />
				<rect x="215" y="248" width="10" height="7" rx="1.5" fill="#3d5bff" opacity="0.45" />
			</g>
			<!-- end rigging group -->

			<!-- ─── Compass rose (bottom-left corner) ─────────────────── -->
			<g transform="translate(52, 395)" opacity={compassOp}>
				<circle cx="0" cy="0" r="20" stroke="rgba(255,255,255,0.28)" stroke-width="1" fill="none" />
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
				<!-- Intercardinal ticks -->
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
				<circle cx="0" cy="0" r="2.5" fill="rgba(255,255,255,0.38)" />
				<text
					x="0"
					y="-24"
					text-anchor="middle"
					fill="rgba(255,255,255,0.45)"
					font-size="7"
					font-family="inherit">N</text
				>
			</g>

			<!-- ─── Seagulls ───────────────────────────────────────────── -->
			{#if gull1.vis}
				<g transform="translate({gull1.x}, {gull1.y})">
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
				<g transform="translate({gull2.x}, {gull2.y})">
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

		<!-- ═══ LAYER 2: BotFace crew + pennant (HTML, z=2) ═══ -->
		<!-- Applies the same bob transform as the ship hull below -->
		<div
			class="crew-layer"
			style:transform="translateY({shipY}px) rotate({shipRot}deg)"
			style:transform-origin="46.1% 59.1%"
		>
			<!-- ShipFlag pennant at main mast top (globe + straw hat mark) -->
			<div
				class="pennant"
				style:left="44.2%"
				style:top="16.5%"
				style:color="rgba(255,255,255,0.82)"
				style="--color-accent:#ffd070"
				style:transform="translateX(2px) translateY(-100%) rotate({pennantAngle}deg)"
			>
				<ShipFlag size={22} />
			</div>

			<!-- Crew 0: front (near bow), hops with follow-through -->
			<div
				class="crew-face"
				style:left="60.2%"
				style:top="53.2%"
				style:transform="translateX(-50%) translateY(-60%) translateY({crew0extra}px)"
			>
				<BotFace seed="ship-a" state="idle" size={46} />
			</div>

			<!-- Crew 1: waver (excited during intro beat) -->
			<div
				class="crew-face"
				style:left="53.9%"
				style:top="53.6%"
				style:transform="translateX(-50%) translateY(-60%)"
			>
				<BotFace seed="ship-b" state={crew1State} size={44} />
			</div>

			<!-- Crew 2: lookout (working / scanning eyes like a spyglass) -->
			<div
				class="crew-face"
				style:left="46.6%"
				style:top="54.1%"
				style:transform="translateX(-50%) translateY(-60%)"
			>
				<BotFace seed="ship-c" state="working" size={40} />
			</div>

			<!-- Crew 3: sleeper (z's, near stern) -->
			<div
				class="crew-face"
				style:left="39.8%"
				style:top="54.5%"
				style:transform="translateX(-50%) translateY(-60%)"
			>
				<BotFace seed="ship-d" state="offline" size={38} />
			</div>
		</div>

		<!-- ═══ LAYER 3: foreground SVG (hull occludes crew bottoms · near waves) ═══ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="3">
			<defs>
				<linearGradient id="bai-hull" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#1e2d54" />
					<stop offset="100%" stop-color="#0e1a36" />
				</linearGradient>
			</defs>

			<!-- Hull body + deck (bobs with the same transform as rigging) -->
			<g transform="translate(0, {shipY}) rotate({shipRot}, 295, 260)">
				<!-- Main hull body -->
				<path
					d="M 185 264
               Q 232 256 295 254
               Q 352 256 418 263
               L 426 274
               Q 416 306 396 319
               Q 352 328 295 329
               Q 238 328 212 319
               Q 190 308 185 282
               Z"
					fill="url(#bai-hull)"
				/>

				<!-- Hull Atlas-blue accent stripe (waterline) -->
				<path
					d="M 188 274 Q 235 268 295 266 Q 350 268 416 274"
					stroke="#3d5bff"
					stroke-width="2.5"
					fill="none"
					opacity="0.72"
				/>

				<!-- Deck surface (slightly lighter top face) -->
				<path
					d="M 185 264
               Q 232 256 295 254
               Q 352 256 418 263
               L 420 272
               Q 352 264 295 262
               Q 232 264 188 272
               Z"
					fill="#1e2f5a"
				/>

				<!-- Deck rail cap line -->
				<path
					d="M 185 264 Q 232 256 295 254 Q 352 256 418 263"
					stroke="#4a5e92"
					stroke-width="1.5"
					fill="none"
				/>
			</g>

			<!-- ─── Wave band 2 ─────────────────────────────────────────── -->
			<path d={W2} fill="#163568" class="wave2" class:paused={reducedMotion} />
			<path
				d={W2top}
				stroke="rgba(255,255,255,0.10)"
				stroke-width="1.5"
				fill="none"
				class="wave2"
				class:paused={reducedMotion}
			/>

			<!-- ─── Wave band 1 (nearest / largest) ───────────────────── -->
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
	/* ─── Scene shell ───────────────────────────────────────────── */
	.ship-scene {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
	}

	/* Fixed-ratio stage: SVG viewBox and % crew positions both refer to 640×440 */
	.stage {
		position: relative;
		width: min(92%, 640px);
		aspect-ratio: 640 / 440;
		flex-shrink: 0;
	}

	/* Both SVG layers fill the stage exactly */
	.layer-svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		overflow: visible;
		pointer-events: none;
	}

	/* ─── Crew HTML layer ───────────────────────────────────────── */
	.crew-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
		will-change: transform;
	}

	.crew-face {
		position: absolute;
		/* fine-tuned in inline style; transform centers and lifts above rail */
	}

	.pennant {
		position: absolute;
		transform-origin: 0 100%;
		will-change: transform;
	}

	/* ─── Wave CSS animations (continuous drift — linear is correct here) ─── */
	@keyframes wave-scroll {
		from {
			transform: translateX(0);
		}
		to {
			transform: translateX(-640px);
		}
	}

	.wave4 {
		animation: wave-scroll 20s linear infinite;
	}
	.wave3 {
		animation: wave-scroll 12s linear infinite;
	}
	.wave2 {
		animation: wave-scroll 7.5s linear infinite;
	}
	.wave1 {
		animation: wave-scroll 4.4s linear infinite;
	}

	/* Offset phases so no two bands start in lockstep */
	.wave3 {
		animation-delay: -3s;
	}
	.wave2 {
		animation-delay: -1.5s;
	}
	.wave1 {
		animation-delay: -0.8s;
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
