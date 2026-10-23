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
	import ShipBg from "#lib/bots/scene/ShipBg.svelte";
	import ShipFg from "#lib/bots/scene/ShipFg.svelte";

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

			<!-- ══ SHIP (bg layer: masts · sails · rigging · wheel · lanterns) ══════ -->
			<ShipBg {shipY} {shipRot} {sailBillow} />

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
			<!-- ══ SHIP (fg layer: hull · castle · cannon ports · crow's-nest rim) ══ -->
			<ShipFg {shipY} {shipRot} />

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
