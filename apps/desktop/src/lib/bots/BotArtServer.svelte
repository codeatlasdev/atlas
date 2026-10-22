<!--
  BotArtServer — step 2 illustration.
  Server (left) ⇄ Mac (right): packets flow both ways, Mac dims/sleeps while server stays lit.
  A "Reviewer · working" status chip with typing dots stays on the server side.
  JS timeline (7 s loop); pauses when document is hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";

	const PERIOD = 7000;
	let t = $state(reducedMotion ? 0.45 : 0);
	let paused = $state(false);

	// Eased segment: maps t∈[a,b] → [0,1] with ease-in-out
	function seg(x: number, a: number, b: number): number {
		const p = Math.max(0, Math.min(1, (x - a) / (b - a)));
		return p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2;
	}

	// Mac dims between t=0.3–0.5, stays dim 0.5–0.82, brightens 0.82–1
	const macOpacity = $derived(
		t < 0.3 ? 1 : t < 0.5 ? 1 - seg(t, 0.3, 0.5) * 0.58 : t < 0.82 ? 0.42 : 0.42 + seg(t, 0.82, 1.0) * 0.58,
	);

	// Mac screen glow dims too
	const macScreenOpacity = $derived(macOpacity * 0.85);

	// Server LED blink: fast pulse when working
	const ledPulse = $derived(0.55 + 0.45 * Math.abs(Math.sin(t * Math.PI * 14)));

	// 3 staggered packets traveling server→mac (offset by 1/3 of period each)
	function packetProgress(phaseOffset: number): number {
		const tLocal = (((t - phaseOffset) % 1) + 1) % 1;
		if (tLocal > 0.2) return -1;
		return tLocal / 0.2;
	}

	// Cubic bezier point: P0=(130,140), P1=(210,95), P2=(290,95), P3=(370,140)
	function bezierPt(p: number): { x: number; y: number } {
		const mt = 1 - p;
		return {
			x: mt ** 3 * 130 + 3 * mt ** 2 * p * 210 + 3 * mt * p ** 2 * 290 + p ** 3 * 370,
			y: mt ** 3 * 140 + 3 * mt ** 2 * p * 95 + 3 * mt * p ** 2 * 95 + p ** 3 * 140,
		};
	}

	const PACKET_OFFSETS = [0, 0.33, 0.67] as const;

	onMount(() => {
		if (reducedMotion) return;
		const startTime = performance.now();
		const id = setInterval(() => {
			if (paused) return;
			t = ((performance.now() - startTime) % PERIOD) / PERIOD;
		}, 50);
		const onVis = () => {
			paused = document.hidden;
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			clearInterval(id);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

<div class="art-server" aria-hidden="true">
	<svg class="scene" viewBox="0 0 500 280" xmlns="http://www.w3.org/2000/svg">
		<!-- ─── Link curve ─────────────────────────────────── -->
		<path
			d="M 130 140 C 210 95 290 95 370 140"
			stroke="var(--ca-border-subtle)"
			stroke-width="1.5"
			stroke-dasharray="5 5"
			fill="none"
			opacity="0.6"
		/>

		<!-- ─── Packets ───────────────────────────────────── -->
		{#each PACKET_OFFSETS as offset, i}
			{@const prog = packetProgress(offset)}
			{#if prog >= 0}
				{@const pt = bezierPt(prog)}
				<circle
					cx={pt.x}
					cy={pt.y}
					r="4.5"
					fill="var(--ca-accent, #4f8ef7)"
					opacity={prog < 0.12 ? prog / 0.12 : prog > 0.88 ? (1 - prog) / 0.12 : 1}
				/>
			{/if}
		{/each}

		<!-- ─── Server (left) ─────────────────────────────── -->
		<rect x="60" y="95" width="140" height="90" rx="8" fill="var(--ca-surface-sunken)" />
		<rect
			x="60"
			y="95"
			width="140"
			height="90"
			rx="8"
			fill="none"
			stroke="var(--ca-border-subtle)"
			stroke-width="1.5"
		/>
		<!-- Drive bays -->
		<rect x="72" y="110" width="80" height="7" rx="3" fill="var(--ca-border-subtle)" opacity="0.7" />
		<rect x="72" y="123" width="80" height="7" rx="3" fill="var(--ca-border-subtle)" opacity="0.7" />
		<rect x="72" y="136" width="80" height="7" rx="3" fill="var(--ca-border-subtle)" opacity="0.7" />
		<!-- LED dots -->
		<circle cx="168" cy="113" r="3" fill="#22c55e" opacity={ledPulse} />
		<circle cx="168" cy="126" r="3" fill="#22c55e" opacity={ledPulse * 0.7} />
		<circle cx="168" cy="139" r="3" fill="var(--ca-border-subtle)" opacity="0.5" />
		<!-- Label -->
		<text
			x="130"
			y="198"
			text-anchor="middle"
			fill="var(--ca-text-secondary)"
			font-size="11"
			font-family="inherit">server</text
		>

		<!-- ─── Status chip ───────────────────────────────── -->
		<rect
			x="52"
			y="207"
			width="156"
			height="26"
			rx="6"
			fill="var(--ca-surface-base)"
			stroke="var(--ca-border-subtle)"
			stroke-width="1"
		/>
		<!-- Pulsing green dot -->
		<circle cx="67" cy="220" r="4" fill="#22c55e" opacity={0.6 + 0.4 * Math.abs(Math.sin(t * Math.PI * 6))} />
		<text
			x="75"
			y="224"
			fill="var(--ca-text-primary)"
			font-size="10.5"
			font-family="inherit"
			font-weight="500">Reviewer</text
		>
		<text x="127" y="224" fill="var(--ca-text-secondary)" font-size="10.5" font-family="inherit">
			· working</text
		>

		<!-- ─── Mac (right) ───────────────────────────────── -->
		<g opacity={macOpacity}>
			<!-- Screen -->
			<rect x="305" y="88" width="130" height="88" rx="7" fill="var(--ca-surface-sunken)" />
			<rect
				x="305"
				y="88"
				width="130"
				height="88"
				rx="7"
				fill="none"
				stroke="var(--ca-border-subtle)"
				stroke-width="1.5"
			/>
			<!-- Screen content glow -->
			<rect
				x="315"
				y="98"
				width="110"
				height="68"
				rx="4"
				fill="var(--ca-accent, #4f8ef7)"
				opacity={macScreenOpacity * 0.06}
			/>
			<!-- Fake menu bar -->
			<rect x="315" y="98" width="110" height="9" rx="2" fill="var(--ca-border-subtle)" opacity="0.4" />
			<!-- Fake content lines -->
			<rect x="320" y="115" width="70" height="5" rx="2" fill="var(--ca-border-subtle)" opacity="0.3" />
			<rect x="320" y="126" width="50" height="5" rx="2" fill="var(--ca-border-subtle)" opacity="0.3" />
			<rect x="320" y="137" width="90" height="5" rx="2" fill="var(--ca-border-subtle)" opacity="0.3" />
			<!-- Base / chin -->
			<rect x="318" y="176" width="104" height="6" rx="3" fill="var(--ca-surface-sunken)" />
			<rect x="295" y="182" width="150" height="5" rx="2.5" fill="var(--ca-border-subtle)" opacity="0.5" />
		</g>
		<!-- Mac label (always legible) -->
		<text
			x="370"
			y="198"
			text-anchor="middle"
			fill="var(--ca-text-secondary)"
			font-size="11"
			font-family="inherit"
			opacity={macOpacity}>your mac</text
		>
	</svg>
</div>

<style>
	.art-server {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.scene {
		width: min(460px, 92%);
		height: auto;
		overflow: visible;
	}
</style>
