<!--
  BotArtServer — step 2 onboarding illustration.
  Night harbour: a cosy house on the left shore (the user's Mac) faces the
  ship at sea (the server).  Over ~10 s the house window dims and switches
  off (Mac sleeps) while the ship keeps sailing — lanterns lit, crew working.
  The story: "runs on your own server, the Mac can sleep."

  Same world and technique as BotArtIntro: full-bleed scene, .ship-scene +
  .sea + .stage (100cqw, 640×440 viewBox), single rAF clock, eased segments,
  reduced-motion still frame, pause when hidden.
  Ship rendered via shared ShipBg / ShipFg components (dx=90 shifts it right).
-->
<script lang="ts">
	import { onMount } from "svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import ShipBg from "#lib/bots/scene/ShipBg.svelte";
	import ShipFg from "#lib/bots/scene/ShipFg.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	// ─── Ship is offset 90 px to the right vs step 1 ───────────────────────────
	const SHIP_DX = 90;

	// ─── Wave geometry (same tile formula as BotArtIntro) ───────────────────────
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

	const W4 = waveD(300, 4);
	const W3 = waveD(316, 8);
	const W2 = waveD(336, 12);
	const W1 = waveD(356, 17);
	const W1L = waveLine(356, 17);
	const W2L = waveLine(336, 12);

	// ─── Animation state ────────────────────────────────────────────────────────

	const INTRO_MS = 10_000;
	// Reduced-motion: skip to steady state (Mac asleep, ship sailing).
	let t = $state(reducedMotion ? 1 : 0);
	let ph = $state(reducedMotion ? 1.1 : 0);
	let paused = $state(false);

	// ─── Easing ─────────────────────────────────────────────────────────────────

	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// ─── Ship dynamics (same frequencies as BotArtIntro) ────────────────────────

	const shipY = $derived(Math.sin(ph) * 7);
	const shipRot = $derived(Math.cos(ph * 1.07) * 2.2);

	// Stage scale factor — keeps crew faces proportional to the ship.
	let stageW = $state(640);
	const k = $derived(Math.max(0.6, stageW / 640));

	// Idle sail billow — no intro spring needed for step 2.
	const sailBillow = $derived(0.88 + Math.sin(ph * 0.58) * 0.07 + Math.sin(ph * 1.31) * 0.03);

	// ─── Lantern glow ────────────────────────────────────────────────────────────

	const lanternPulse = $derived(0.55 + 0.45 * Math.abs(Math.sin(ph * 2.1)));

	// ─── Timeline ────────────────────────────────────────────────────────────────
	// t=0→0.18  : moon fades in
	// t=0.18→0.4: house window dims (Mac going to sleep)
	// t=0.4→1   : Mac stays asleep; ship sails on; "z" drifts from chimney
	// t=1+       : calm loop — ph keeps advancing, t stays 1

	const moonOp = $derived(t < 0.18 ? seg(t, 0, 0.18) : 1);

	// Window glow: 1 = fully lit, 0 = off.
	const winGlow = $derived(t < 0.18 ? 1 : t < 0.42 ? 1 - seg(t, 0.18, 0.42) : 0);

	// Stars twinkle.
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

	// "z" drifting from the chimney: appears once the Mac is asleep.
	// Driven by ph so it keeps looping after the intro ends.
	const zVisible = $derived(t < 0.38 ? 0 : t < 0.5 ? seg(t, 0.38, 0.5) : 1);
	const zOp = $derived(zVisible * 0.72 * Math.max(0, 0.5 + 0.5 * Math.sin(ph * 0.9)));
	const zY = $derived(178 - (0.5 + 0.5 * Math.sin(ph * 0.9 + 1.0)) * 22);

	// Dotted link from house to ship: fades with winGlow (dims as Mac sleeps).
	const linkOp = $derived(winGlow * 0.85);

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
	<!-- Sky above the stage and sea below it — fills the whole column. -->
	<div class="sea" style="--sky:#08102a; --deep:#1b3e7a"></div>
	<div class="stage" bind:clientWidth={stageW}>
		<!-- ═══ LAYER 1 — bg SVG (z=1): sky · water · shore · house · ship bg ══════ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="1">
			<defs>
				<linearGradient id="bas-sky" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#08102a" />
					<stop offset="40%" stop-color="#111e50" />
					<stop offset="78%" stop-color="#1a3272" />
					<stop offset="100%" stop-color="#1f3a7a" />
				</linearGradient>
				<radialGradient id="bas-glow" cx="50%" cy="100%" r="68%">
					<stop offset="0%" stop-color="#cc5f18" stop-opacity="0.30" />
					<stop offset="55%" stop-color="#cc5f18" stop-opacity="0.07" />
					<stop offset="100%" stop-color="#cc5f18" stop-opacity="0" />
				</radialGradient>
				<radialGradient id="bas-moonh" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#fff8e4" stop-opacity="0.28" />
					<stop offset="55%" stop-color="#ffcc60" stop-opacity="0.10" />
					<stop offset="100%" stop-color="#ffcc60" stop-opacity="0" />
				</radialGradient>
				<linearGradient id="bas-water" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0a1e42" />
					<stop offset="100%" stop-color="#050e1e" />
				</linearGradient>
				<linearGradient id="bas-refl" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#ffcc60" stop-opacity="0.22" />
					<stop offset="100%" stop-color="#ffcc60" stop-opacity="0" />
				</linearGradient>
				<!-- Warm lantern reflection on the water behind the ship's stern -->
				<radialGradient id="bas-lantrefl" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#ffb030" stop-opacity="0.55" />
					<stop offset="100%" stop-color="#ff6600" stop-opacity="0" />
				</radialGradient>
				<!-- House window warm glow halo -->
				<radialGradient id="bas-winlight" cx="50%" cy="50%" r="50%">
					<stop offset="0%" stop-color="#ffd880" stop-opacity="1" />
					<stop offset="100%" stop-color="#ff9920" stop-opacity="0" />
				</radialGradient>
				<!-- Shore ground gradient: dark earthy green -->
				<linearGradient id="bas-shore" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0%" stop-color="#0c1e0e" />
					<stop offset="100%" stop-color="#070f08" />
				</linearGradient>
			</defs>

			<!-- Sky + horizon glow -->
			<rect width="640" height="440" fill="url(#bas-sky)" />
			<rect width="640" height="440" fill="url(#bas-glow)" />

			<!-- Stars -->
			{#each STARS as s}
				<circle cx={s.x} cy={s.y} r={s.r} fill="white" opacity={reducedMotion ? 0.65 : starOp(s.p)} />
			{/each}

			<!-- Moon (upper-right, above the ship side) -->
			<g transform="translate(522, 68)" opacity={moonOp}>
				<circle r="58" fill="url(#bas-moonh)" />
				<circle r="22" fill="#fff5da" />
				<circle cx="6" cy="-5" r="5" fill="rgba(196,172,108,0.18)" />
				<circle cx="-8" cy="7" r="3.5" fill="rgba(196,172,108,0.13)" />
			</g>

			<!-- Water base + moon reflection -->
			<rect x="0" y="290" width="640" height="150" fill="url(#bas-water)" />
			<rect x="502" y="290" width="42" height="150" fill="url(#bas-refl)" opacity={moonOp} />

			<!-- Far waves (behind everything) -->
			<path d={W4} fill="#0d2248" class="wave4" class:paused={reducedMotion} />
			<path d={W3} fill="#112a56" class="wave3" class:paused={reducedMotion} />

			<!-- ── Shore (left side) ───────────────────────────────────────────── -->
			<!-- Land mass: gentle hill tapering to the waterline at x≈200 -->
			<path
				d="M 0 440 L 0 272 Q 30 265 68 267 Q 108 269 142 277 Q 170 284 192 296 Q 208 306 214 322 L 214 440 Z"
				fill="url(#bas-shore)"
			/>
			<!-- Shore edge highlight (soft grassy rim) -->
			<path
				d="M 0 272 Q 30 265 68 267 Q 108 269 142 277 Q 170 284 192 296 Q 208 306 214 322"
				stroke="#1a3020"
				stroke-width="2.5"
				fill="none"
				opacity="0.7"
			/>
			<!-- Small dock extending into water -->
			<rect x="156" y="285" width="52" height="6" rx="2" fill="#0c1c10" opacity="0.85" />
			<line x1="160" y1="285" x2="160" y2="292" stroke="#162818" stroke-width="2" />
			<line x1="172" y1="285" x2="172" y2="292" stroke="#162818" stroke-width="2" />
			<line x1="184" y1="285" x2="184" y2="292" stroke="#162818" stroke-width="2" />
			<line x1="196" y1="285" x2="196" y2="292" stroke="#162818" stroke-width="2" />

			<!-- ── House ───────────────────────────────────────────────────────── -->
			<!-- Body -->
			<rect x="45" y="218" width="82" height="62" rx="3" fill="#0d1c34" />
			<!-- Roof -->
			<path d="M 39 220 L 86 175 L 133 220 Z" fill="#0a1525" />
			<!-- Roof ridge highlight -->
			<path d="M 40 219 L 86 175 L 132 219" stroke="#1a2848" stroke-width="1.5" fill="none" />
			<!-- Chimney -->
			<rect x="103" y="183" width="11" height="30" rx="1" fill="#0c1a2e" />
			<rect x="101" y="181" width="15" height="5" rx="1" fill="#0e1e38" />
			<!-- Door -->
			<rect x="84" y="258" width="15" height="22" rx="2" fill="#060e1e" />
			<circle cx="97" cy="269" r="1.5" fill="#1a2848" />
			<!-- Window frame -->
			<rect x="56" y="232" width="26" height="20" rx="2" fill="#060c1e" stroke="#1a2848" stroke-width="1" />
			<!-- Second window (right side) -->
			<rect x="92" y="232" width="20" height="16" rx="2" fill="#060c1e" stroke="#1a2848" stroke-width="1" />

			<!-- Window warm glow (dims as Mac sleeps) -->
			<rect x="57" y="233" width="24" height="18" rx="1.5" fill="rgba(255,216,128,{winGlow * 0.88})" />
			<rect x="93" y="233" width="18" height="14" rx="1.5" fill="rgba(255,200,100,{winGlow * 0.65})" />
			<!-- Window halo glow spilling outward -->
			<circle cx="69" cy="242" r="32" fill="url(#bas-winlight)" opacity={winGlow * 0.55} />

			<!-- "z" drifting from chimney when Mac is sleeping -->
			<text
				x="109"
				y={zY}
				text-anchor="middle"
				font-size="11"
				font-style="italic"
				font-family="inherit"
				fill="rgba(180,210,255,0.9)"
				opacity={zOp}>z</text
			>

			<!-- ── Dotted link: house → ship (fades when Mac sleeps) ─────────── -->
			<path
				d="M 148 255 Q 192 268 242 280"
				stroke="rgba(255,255,255,0.55)"
				stroke-width="1.3"
				stroke-dasharray="4 6"
				fill="none"
				opacity={linkOp}
			/>

			<!-- ── Ship wake trail (behind stern, shifted with ship dx=90) ────── -->
			<g transform="translate(0,{shipY})">
				<path
					d="M 238 306 Q 180 314 126 302 Q 102 298 92 294"
					stroke="rgba(255,255,255,0.13)"
					stroke-width="3.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 7"
				/>
				<path
					d="M 238 316 Q 170 328 118 318 Q 98 314 91 308"
					stroke="rgba(255,255,255,0.07)"
					stroke-width="2.5"
					stroke-linecap="round"
					fill="none"
					stroke-dasharray="1 10"
				/>
			</g>

			<!-- Lantern warmth on the water: soft glow behind the ship's stern -->
			<ellipse cx="234" cy="302" rx="44" ry="11" fill="url(#bas-lantrefl)" opacity={lanternPulse * 0.5} />

			<!-- ══ SHIP bg layer (dx=90: centred in right half) ═════════════════ -->
			<ShipBg dx={SHIP_DX} {shipY} {shipRot} {sailBillow} />
		</svg>

		<!-- ═══ LAYER 2 — crew HTML (z=2): above sails, below hull rail ══════════
             Two crew: crow's-nest lookout + helmsman.
             left% = (originalX + SHIP_DX) / 640 × 100
               Crow's nest: (308+90)/640 = 62.2%
               Helmsman:    (192+90)/640 = 44.1%
		-->
		<div
			class="crew-layer"
			style:transform="translateY({shipY}px) rotate({shipRot}deg)"
			style:transform-origin="{((308 + SHIP_DX) / 640) * 100}% 64.8%"
		>
			<!-- Crow's-nest lookout with spyglass -->
			<div
				class="crew-face"
				style:left="62.2%"
				style:top="17.7%"
				style:transform="translateX(-50%) translateY(-50%)"
			>
				<BotFace seed="srv-crow" state="working" size={Math.round(42 * k)} />
			</div>

			<!-- Helmsman at the steering wheel -->
			<div
				class="crew-face"
				style:left="44.1%"
				style:top="60.9%"
				style:transform="translateX(-50%) translateY(-100%)"
			>
				<BotFace seed="srv-helm" state="working" size={Math.round(60 * k)} />
			</div>
		</div>

		<!-- ═══ LAYER 3 — fg SVG (z=3): hull occludes crew feet · near waves ══════ -->
		<svg class="layer-svg" viewBox="0 0 640 440" xmlns="http://www.w3.org/2000/svg" style:z-index="3">
			<!-- ══ SHIP fg layer (hull · castle · cannon ports · crow's-nest rim) ══ -->
			<ShipFg dx={SHIP_DX} {shipY} {shipRot} />

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

	/* Same sea fill formula as BotArtIntro. */
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

	/* Fixed-ratio stage: crew % positions align with SVG 640×440 viewBox. */
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

	/* Wave CSS drift — linear for continuous translation. */
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
