<script lang="ts" module>
	let uid = 0;
</script>

<script lang="ts">
	import { onMount } from "svelte";
	import BotHat from "#lib/bots/BotHat.svelte";
	import type { BotState } from "#lib/bots/botFace.ts";
	import { normalizeLook, shade } from "#lib/bots/look.ts";
	import { SHELL_PATHS } from "#lib/bots/shell.ts";

	let {
		seed,
		state: botState,
		size = 36,
		look,
		track = false,
	}: {
		seed: string;
		state: BotState;
		size?: number;
		/** Customised look, possibly straight off the wire; invalid fields fall back to the seed's look. */
		look?: unknown;
		/** Eyes follow the pointer (editor preview). */
		track?: boolean;
	} = $props();

	const id = `bot${++uid}`;
	const l = $derived(normalizeLook(look, seed));
	const asleep = $derived(botState === "offline");
	const ink = "#14142b";

	// Gaze in [-1, 1]; pupils ease toward it through a CSS transition.
	let gx = $state(0);
	let gy = $state(0);
	let blinking = $state(false);
	let paused = $state(false);
	let svg: SVGSVGElement;

	onMount(() => {
		const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
		const timers = new Set<ReturnType<typeof setTimeout>>();
		const later = (ms: number, fn: () => void) => {
			const t = setTimeout(() => {
				timers.delete(t);
				fn();
			}, ms);
			timers.add(t);
		};
		let lastPointer = 0;

		// Blink at irregular intervals, now and then twice: a fixed loop reads as a machine.
		const blink = () => {
			if (!paused && !asleep) {
				blinking = true;
				later(130, () => (blinking = false));
				if (Math.random() < 0.2) {
					later(260, () => (blinking = true));
					later(390, () => (blinking = false));
				}
			}
			later(1800 + Math.random() * 3800, blink);
		};

		// Saccades: idle glances around, working scans like reading, needs_you stares at you.
		let side = 1;
		const glance = () => {
			const recentPointer = track && performance.now() - lastPointer < 2500;
			if (!paused && !recentPointer) {
				if (botState === "working") {
					side = -side;
					gx = side * 0.7;
					gy = 0.25;
				} else if (botState === "needs_you" || Math.random() < 0.35) {
					gx = 0;
					gy = 0;
				} else {
					gx = Math.random() * 1.6 - 0.8;
					gy = Math.random() * 1.2 - 0.6;
				}
			}
			later(botState === "working" ? 700 + Math.random() * 500 : 1200 + Math.random() * 2600, glance);
		};

		const onPointer = (e: PointerEvent) => {
			const r = svg.getBoundingClientRect();
			const dx = e.clientX - (r.left + r.width / 2);
			const dy = e.clientY - (r.top + r.height / 2);
			const reach = Math.max(r.width, 240);
			gx = Math.max(-1, Math.min(1, dx / reach));
			gy = Math.max(-1, Math.min(1, dy / reach));
			lastPointer = performance.now();
		};
		const onVisibility = () => (paused = document.hidden);

		if (!reduce) {
			later(600 + Math.random() * 2000, blink);
			later(800 + Math.random() * 1500, glance);
		}
		if (track) window.addEventListener("pointermove", onPointer);
		document.addEventListener("visibilitychange", onVisibility);
		return () => {
			for (const t of timers) clearTimeout(t);
			window.removeEventListener("pointermove", onPointer);
			document.removeEventListener("visibilitychange", onVisibility);
		};
	});

	const px = $derived(gx * 14);
	const py = $derived(gy * 10);
</script>

<svg
	bind:this={svg}
	viewBox="-90 -170 608 608"
	width={size}
	height={size}
	aria-hidden="true"
	class:paused
	class:asleep
	xmlns="http://www.w3.org/2000/svg"
>
	<defs>
		<radialGradient id="{id}-g" cx="38%" cy="30%" r="75%">
			<stop offset="0" stop-color={shade(l.color, 0.35)} />
			<stop offset=".55" stop-color={l.color} />
			<stop offset="1" stop-color={shade(l.color, -0.35)} />
		</radialGradient>
	</defs>

	<ellipse class="shadow" cx="214" cy="432" rx="150" ry="16" fill="#000" opacity=".22" />

	<g class="body body-{botState}">
		<circle cx="214" cy="213" r="206" fill="url(#{id}-g)" />
		{#if size >= 40}
			<g fill={shade(l.color, 0.22)} opacity=".55">
				{#each SHELL_PATHS as d (d)}<path {d} />{/each}
			</g>
		{/if}

		<!-- face -->
		<ellipse cx="146" cy="250" rx="26" ry="15" fill="#ff7aa8" opacity=".35" />
		<ellipse cx="282" cy="250" rx="26" ry="15" fill="#ff7aa8" opacity=".35" />

		<g class="eyes" class:blink={blinking}>
			{#if asleep}
				<path
					d="M140 196 Q170 214 200 196 M228 196 Q258 214 288 196"
					stroke={ink}
					stroke-width="12"
					stroke-linecap="round"
					fill="none"
				/>
			{:else if l.eyes === "round" || l.eyes === "sleepy"}
				{#each [170, 258] as cx (cx)}
					<circle {cx} cy="195" r="36" fill="#fff" />
					<g class="pupil" style:transform="translate({px}px, {py + (l.eyes === 'sleepy' ? 8 : 0)}px)">
						<circle {cx} cy="198" r="18" fill={ink} />
						<circle cx={cx + 7} cy="190" r="6" fill="#fff" />
					</g>
				{/each}
				{#if l.eyes === "sleepy"}
					<path d="M128 190 Q170 146 212 190 Z M216 190 Q258 146 300 190 Z" fill={shade(l.color, -0.1)} />
				{/if}
			{:else if l.eyes === "dots"}
				<g class="pupil" style:transform="translate({px}px, {py}px)">
					{#each [174, 254] as cx (cx)}
						<ellipse {cx} cy="196" rx="19" ry="26" fill={ink} />
						<circle cx={cx + 6} cy="186" r="7" fill="#fff" />
					{/each}
				</g>
			{:else if l.eyes === "happy"}
				<g class="pupil" style:transform="translate({px * 0.6}px, {py * 0.6}px)">
					<path
						d="M144 206 Q172 166 200 206 M228 206 Q256 166 284 206"
						stroke={ink}
						stroke-width="14"
						stroke-linecap="round"
						fill="none"
					/>
				</g>
			{:else if l.eyes === "visor"}
				<rect x="112" y="160" width="204" height="72" rx="36" fill={ink} />
				<g class="pupil" style:transform="translate({px * 1.6}px, {py * 0.8}px)">
					<rect x="150" y="178" width="30" height="36" rx="12" fill={shade(l.color, 0.6)} />
					<rect x="248" y="178" width="30" height="36" rx="12" fill={shade(l.color, 0.6)} />
				</g>
			{:else if l.eyes === "cyclops"}
				<circle cx="214" cy="192" r="54" fill="#fff" />
				<g class="pupil" style:transform="translate({px * 1.4}px, {py * 1.2}px)">
					<circle cx="214" cy="196" r="26" fill={ink} />
					<circle cx="224" cy="184" r="8" fill="#fff" />
				</g>
			{/if}
		</g>

		{#if botState === "needs_you"}
			<ellipse cx="214" cy="272" rx="14" ry="17" fill={ink} />
		{:else if asleep}
			<path d="M198 268 L230 268" stroke={ink} stroke-width="10" stroke-linecap="round" />
		{:else}
			<path d="M190 262 Q214 286 238 262" stroke={ink} stroke-width="10" stroke-linecap="round" fill="none" />
		{/if}

		<g class="hat"><BotHat hat={l.hat} color={l.color} /></g>
	</g>

	{#if asleep}
		<g fill="currentColor" font-family="ui-rounded, system-ui" font-weight="700">
			<text class="z z1" x="390" y="40" font-size="70">z</text>
			<text class="z z2" x="440" y="-20" font-size="54">z</text>
			<text class="z z3" x="480" y="-70" font-size="40">z</text>
		</g>
	{/if}
</svg>

<style>
	svg {
		display: block;
		flex: none;
		overflow: visible;
	}
	.asleep .body {
		filter: grayscale(0.85);
		opacity: 0.6;
	}

	/* Squash-and-stretch from the floor so it reads as a soft, living body. */
	.body {
		transform-origin: 214px 426px;
	}
	.body-idle {
		animation: breathe 3.6s ease-in-out infinite;
	}
	.body-offline {
		animation: breathe 5.5s ease-in-out infinite;
	}
	@keyframes breathe {
		0%,
		100% {
			transform: scale(1, 1);
		}
		50% {
			transform: scale(1.015, 1.035);
		}
	}
	.body-working {
		animation: bob 1.4s ease-in-out infinite;
	}
	@keyframes bob {
		0%,
		100% {
			transform: translateY(0) rotate(0deg);
		}
		25% {
			transform: translateY(-10px) rotate(-1.5deg);
		}
		75% {
			transform: translateY(-10px) rotate(1.5deg);
		}
	}
	.body-needs_you {
		animation: hop 0.9s cubic-bezier(0.3, 0, 0.3, 1) infinite;
	}
	@keyframes hop {
		0%,
		100% {
			transform: translateY(0) scale(1.06, 0.94);
		}
		40% {
			transform: translateY(-46px) scale(0.96, 1.05);
		}
		70% {
			transform: translateY(0) scale(1.04, 0.96);
		}
	}
	.shadow {
		transform-origin: 214px 432px;
	}
	svg:has(.body-needs_you) .shadow {
		animation: shadow 0.9s cubic-bezier(0.3, 0, 0.3, 1) infinite;
	}
	@keyframes shadow {
		40% {
			transform: scale(0.7);
			opacity: 0.12;
		}
	}

	/* Hat lags the body a beat: secondary motion. */
	.hat {
		transform-origin: 214px 40px;
		animation: sway 3.6s 0.25s ease-in-out infinite;
	}
	.body-needs_you .hat {
		animation: flop 0.9s 0.08s ease-in-out infinite;
	}
	@keyframes sway {
		0%,
		100% {
			transform: rotate(-1.5deg);
		}
		50% {
			transform: rotate(1.5deg);
		}
	}
	@keyframes flop {
		0%,
		100% {
			transform: translateY(0) rotate(0deg);
		}
		45% {
			transform: translateY(-14px) rotate(-5deg);
		}
		75% {
			transform: translateY(4px) rotate(3deg);
		}
	}

	.eyes {
		transform-origin: 214px 196px;
		transition: transform 70ms ease-in;
	}
	.eyes.blink {
		transform: scaleY(0.08);
	}
	.pupil {
		transition: transform 160ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}

	.z {
		opacity: 0;
		animation: zfloat 3s ease-out infinite;
	}
	.z2 {
		animation-delay: 0.9s;
	}
	.z3 {
		animation-delay: 1.8s;
	}
	@keyframes zfloat {
		0% {
			transform: translateY(0);
			opacity: 0;
		}
		20% {
			opacity: 0.7;
		}
		100% {
			transform: translateY(-60px);
			opacity: 0;
		}
	}

	.paused,
	.paused * {
		animation-play-state: paused !important;
	}
	@media (prefers-reduced-motion: reduce) {
		svg,
		svg * {
			animation: none !important;
			transition: none !important;
		}
	}
</style>
