<!--
  BotArtHow — the one "how it works" screen of the Crew onboarding.

  One night, told in ~14 s with the real pieces of the product: you give an order in the composer, a crewmate takes
  it and works on YOUR server while the sky turns to night and your Mac falls asleep, it stops to ask before
  `git push`, you allow it from your phone, and at dawn the result is waiting. `beat` (0–3) tells the left column
  which of its four lines to light up, so text and picture tell the same story at the same moment.

  One rAF clock, eased segments, holds between beats; reduced motion shows the finished night; paused when hidden.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { Check, GitPullRequest, Laptop, Moon, Server, Smartphone } from "@lucide/svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { normalizeLook } from "#lib/bots/look.ts";

	/** The typing dots take the crewmate's own colour. */
	const crewColor = normalizeLook(undefined, "how-reviewer").color;

	let { beat = $bindable(0) }: { beat?: number } = $props();

	const PERIOD = 22000;
	let t = $state(reducedMotion ? 0.95 : 0);
	let w = $state(640);
	const face = $derived(Math.round(Math.min(120, Math.max(64, w * 0.12))));

	const clamp01 = (x: number) => Math.max(0, Math.min(1, x));
	const ease = (p: number) => (p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2);
	/** 0 → 1 over [a, b], eased. */
	const seg = (a: number, b: number) => ease(clamp01((t - a) / (b - a)));
	const at = (a: number) => t >= a;

	// ── Script ────────────────────────────────────────────────────────────
	const ORDER = "Review every PR on atlas-bot tonight";
	const typed = $derived(ORDER.slice(0, Math.round(seg(0.02, 0.11) * ORDER.length)));
	const sent = $derived(at(0.12));
	const steps = [
		{ at: 0.2, text: "Read PR #115 · 4 files" },
		{ at: 0.28, text: "Ran the tests · 212 passed" },
		{ at: 0.36, text: "Left 2 review comments" },
		{ at: 0.7, text: "Pushed the fix to main" },
	];
	// What the crewmate says, like a friend texting back: typing dots first, then the words arrive.
	const LINES = [
		{ at: 0.15, text: "On it! I'll go through them tonight." },
		{ at: 0.44, text: "Hey, one of these needs a push to main. Okay with you?" },
		{ at: 0.62, text: "Thanks! Pushing it now." },
		{ at: 0.8, text: "Good morning! I reviewed 3 PRs and pushed 1 fix." },
	];
	const TYPE = 0.08;
	/** Every line said so far stays in the thread (a chat keeps its history); only the newest one is still typing. */
	const spoken = $derived(
		LINES.filter((l) => t >= l.at).map((l) => ({
			text: l.text,
			typing: t < l.at + TYPE,
			shown: l.text.slice(0, Math.round(clamp01((t - l.at - TYPE) / 0.05) * l.text.length)),
		})),
	);
	const asking = $derived(at(0.46) && !at(0.6));
	const allowed = $derived(at(0.6));
	const tap = $derived(seg(0.55, 0.6) * (1 - seg(0.6, 0.64)));
	const done = $derived(at(0.8));
	const working = $derived(at(0.15) && !done && !asking);
	const faceState = $derived(done ? "idle" : asking ? "needs_you" : working ? "working" : "idle");

	// Time of day behind it all: evening → night → dawn.
	const night = $derived(seg(0.14, 0.3) * (1 - seg(0.74, 0.86)));
	const dawn = $derived(seg(0.74, 0.86));
	const macAsleep = $derived(at(0.3));

	$effect(() => {
		beat = done ? 3 : t >= 0.44 ? 2 : t >= 0.14 ? 1 : 0;
	});

	onMount(() => {
		if (reducedMotion) return;
		let t0 = performance.now();
		let raf = 0;
		let hiddenAt = 0;
		const tick = (now: number) => {
			t = ((now - t0) % PERIOD) / PERIOD;
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		const onVis = () => {
			if (document.hidden) {
				hiddenAt = performance.now();
				cancelAnimationFrame(raf);
			} else {
				t0 += performance.now() - hiddenAt;
				raf = requestAnimationFrame(tick);
			}
		};
		document.addEventListener("visibilitychange", onVis);
		return () => {
			cancelAnimationFrame(raf);
			document.removeEventListener("visibilitychange", onVis);
		};
	});
</script>

{#snippet say(l: { text: string; typing: boolean; shown: string })}
	{#if l.typing}
		<!-- Typing, like the real chat: no bubble, the dots and the crewmate's little face take turns, then a shimmering label. -->
		<div class="typing pop">
			<span class="mark">
				<span class="dots" style:--crew={crewColor}><i></i><i></i><i></i></span>
				<span class="mini"><BotFace seed="how-reviewer" state="working" size={22} /></span>
			</span>
			<span class="shimmer">Thinking</span>
		</div>
	{:else}
		<div class="bubble bot pop">{l.shown}</div>
	{/if}
{/snippet}

<div class="how" bind:clientWidth={w} aria-hidden="true">
	<!-- Sky: three layers crossfaded, so time visibly passes while the crew works. -->
	<div class="sky evening"></div>
	<div class="sky night" style:opacity={night}></div>
	<div class="sky dawn" style:opacity={dawn}></div>
	<div class="stars" style:opacity={night}></div>
	<svg class="sea" viewBox="0 0 1200 120" preserveAspectRatio="none">
		<path d="M0 40 Q150 20 300 40 T600 40 T900 40 T1200 40 V120 H0 Z" class="wave far" />
		<path d="M0 62 Q150 44 300 62 T600 62 T900 62 T1200 62 V120 H0 Z" class="wave near" />
	</svg>

	<!-- Where it runs, and what your Mac is doing meanwhile. -->
	<div class="chips">
		<span class="chip">
			<Server size={14} />
			<span>your server</span>
			<span class="dot live"></span>
		</span>
		<span class="chip" class:asleep={macAsleep}>
			<Laptop size={14} />
			<span>your Mac</span>
			{#if macAsleep}<span class="sleep"><Moon size={12} /> asleep</span>{/if}
		</span>
	</div>

	<div class="thread">
		<!-- 1. The order -->
		{#if sent}
			<div class="bubble you pop">{ORDER}</div>
		{/if}

		<!-- The crewmate talks back: first reply above the work card, the rest below it, in order. -->
		{#if spoken[0]}{@render say(spoken[0])}{/if}

		<!-- 2. The crewmate takes it -->
		{#if at(0.15)}
			<div class="crew pop">
				<div class="face" class:hop={done}><BotFace seed="how-reviewer" state={faceState} size={face} /></div>
				<div class="card">
					<div class="card-head">
						<b>Reviewer</b>
						{#if done}
							<span class="status ok">done</span>
						{:else if asking}
							<span class="status warn">needs you</span>
						{:else}
							<span class="status shimmer">working…</span>
						{/if}
					</div>
					<div class="event"><GitPullRequest size={13} /> PR #115 opened</div>
					<ul class="steps">
						{#each steps as s (s.text)}
							{#if at(s.at)}
								<li class="step-in"><span class="tick"><Check size={11} /></span>{s.text}</li>
							{/if}
						{/each}
					</ul>

					<!-- 3. It asks before anything risky -->
					{#if asking}
						<div class="ask pop">
							<span>Wants to run</span>
							<code>git push origin main</code>
							<div class="ask-btns">
								<span class="btn">Deny</span>
								<span class="btn primary" style:transform="scale({1 - tap * 0.08})">Allow</span>
							</div>
						</div>
					{:else if allowed}
						<div class="allowed pop"><Smartphone size={13} /> Allowed from your phone</div>
					{/if}
				</div>
			</div>
		{/if}
		{#each spoken.slice(1) as l (l.text)}{@render say(l)}{/each}
	</div>

	<!-- The phone that answers the ask. -->
	<div class="phone" class:in={at(0.47) && !at(0.68)}>
		<div class="notif">
			<b>Reviewer needs you</b>
			<span>git push origin main</span>
			<div class="ask-btns">
				<span class="btn">Deny</span>
				<span class="btn primary">Allow</span>
				<span class="ring" style:opacity={tap} style:transform="scale({0.6 + tap})"></span>
			</div>
		</div>
	</div>

	<!-- The composer the order is typed into. -->
	<div class="composer" class:gone={sent}>
		<span class="typed">{typed}</span><span class="caret"></span>
	</div>
</div>

<style>
	.how {
		position: absolute;
		inset: 0;
		overflow: hidden;
		container-type: size;
		/* Everything below is sized in em off this, so the scene keeps its proportions from laptop to full screen. */
		font-size: clamp(13px, 2.1cqw, 21px);
		/* Fixed night palette: the sky is dark in every theme, so the text on it is too light to follow the theme. */
		--ink: #e8ecff;
		--ink-strong: #fff;
		--ink-soft: #d0d6ff;
		--ink-muted: #a9b3e6;
		--ok: #34d399;
		--warn: #fbbf24;
		color: var(--ink);
	}

	.sky {
		position: absolute;
		inset: 0;
		transition: none;
	}
	.sky.evening {
		background: linear-gradient(to bottom, #1b2350 0%, #3a3266 55%, #8a4b5e 100%);
	}
	.sky.night {
		background: linear-gradient(to bottom, #050a1f 0%, #0c1640 60%, #16245a 100%);
	}
	.sky.dawn {
		background: linear-gradient(to bottom, #2a3a78 0%, #7a6aa0 50%, #f0a868 100%);
	}
	.stars {
		position: absolute;
		inset: 0 0 30% 0;
		background-image:
			radial-gradient(1.5px 1.5px at 12% 18%, #fff 50%, transparent 51%),
			radial-gradient(1px 1px at 28% 8%, #fff 50%, transparent 51%),
			radial-gradient(1.5px 1.5px at 46% 22%, #fff 50%, transparent 51%),
			radial-gradient(1px 1px at 63% 12%, #fff 50%, transparent 51%),
			radial-gradient(1.5px 1.5px at 81% 26%, #fff 50%, transparent 51%),
			radial-gradient(1px 1px at 90% 6%, #fff 50%, transparent 51%),
			radial-gradient(1px 1px at 37% 36%, #fff 50%, transparent 51%);
	}
	.sea {
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		width: 100%;
		height: 16%;
	}
	.wave.far {
		fill: #0d2248;
		opacity: 0.85;
	}
	.wave.near {
		fill: #12306a;
	}

	.chips {
		position: absolute;
		top: 1.4em;
		left: 0;
		right: 0;
		display: flex;
		justify-content: center;
		gap: 0.6em;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 0.45em;
		padding: 0.35em 0.8em;
		border-radius: 999px;
		font-size: 0.72em;
		background: rgb(255 255 255 / 0.08);
		border: 1px solid rgb(255 255 255 / 0.12);
		backdrop-filter: blur(6px);
		transition:
			opacity 600ms ease,
			filter 600ms ease;
	}
	.chip.asleep {
		opacity: 0.55;
		filter: saturate(0.4);
	}
	.sleep {
		display: inline-flex;
		align-items: center;
		gap: 0.25em;
		color: var(--ink-soft);
	}
	.dot.live {
		width: 0.5em;
		height: 0.5em;
		border-radius: 50%;
		background: #34d399;
		box-shadow: 0 0 0 0 rgb(52 211 153 / 0.6);
		animation: live 2s ease-out infinite;
	}
	@keyframes live {
		to {
			box-shadow: 0 0 0 0.5em rgb(52 211 153 / 0);
		}
	}

	.thread {
		position: absolute;
		left: 50%;
		top: 42%;
		translate: -50% -50%;
		width: min(30em, 86%);
		display: flex;
		flex-direction: column;
		gap: 0.8em;
	}

	.bubble {
		padding: 0.65em 1em;
		border-radius: 1.1em;
		line-height: 1.35;
		font-size: 0.86em;
		max-width: 85%;
	}
	.bubble.you {
		align-self: flex-end;
		background: #3d5bff;
		color: var(--ink-strong);
		border-bottom-right-radius: 0.35em;
	}

	.bubble.bot {
		align-self: flex-start;
		margin-left: 4.2em;
		min-height: 1.35em;
		background: rgb(255 255 255 / 0.12);
		border: 1px solid rgb(255 255 255 / 0.14);
		border-bottom-left-radius: 0.35em;
		backdrop-filter: blur(8px);
	}
	.typing {
		align-self: flex-start;
		margin-left: 4.2em;
		display: flex;
		align-items: center;
		gap: 0.55em;
		font-size: 0.86em;
		min-height: 2.1em;
	}
	.mark {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: 2.4em;
		height: 1.6em;
	}
	.mark > * {
		grid-area: 1 / 1;
	}
	/* The dots gather into one point, the crewmate pops out of it, squashes, hops and wiggles, then splits back
	   into dots. One 2.6 s loop, shared by every piece so they stay in step. */
	.mark .dots i {
		opacity: 1;
		animation: 2.6s cubic-bezier(0.45, 0, 0.2, 1) infinite;
	}
	.mark .dots i:nth-child(1) {
		--gx: 0.75em;
		animation-name: gather;
		animation-delay: 0s;
	}
	.mark .dots i:nth-child(2) {
		--gx: 0em;
		animation-name: gather;
		animation-delay: 0s;
		--lift: -0.22em;
	}
	.mark .dots i:nth-child(3) {
		--gx: -0.75em;
		animation-name: gather;
		animation-delay: 0s;
	}
	@keyframes gather {
		0%,
		100% {
			transform: none;
		}
		10% {
			transform: translateY(-0.22em);
		}
		20% {
			transform: none;
		}
		30% {
			transform: translateY(var(--lift, 0));
		}
		38% {
			transform: none;
		}
		46% {
			opacity: 1;
			transform: translateX(var(--gx));
		}
		48%,
		88% {
			opacity: 0;
			transform: translateX(var(--gx));
		}
		95% {
			opacity: 1;
			transform: none;
		}
	}
	.mark .mini {
		display: inline-flex;
		transform: scale(0);
		animation: crew-hop 2.6s cubic-bezier(0.45, 0, 0.2, 1) infinite;
	}
	@keyframes crew-hop {
		0%,
		44% {
			transform: scale(0);
		}
		50% {
			transform: scale(1.3, 0.75);
		}
		56% {
			transform: translateY(-0.35em) scale(0.85, 1.15);
		}
		62% {
			transform: scale(1.1, 0.9);
		}
		68% {
			transform: rotate(-14deg);
		}
		74% {
			transform: rotate(12deg);
		}
		80% {
			transform: rotate(-6deg);
		}
		85% {
			transform: none;
		}
		90%,
		100% {
			transform: scale(0);
		}
	}
	.dots {
		display: inline-flex;
		gap: 0.3em;
		padding: 0.25em 0;
	}
	.dots i {
		width: 0.55em;
		height: 0.55em;
		border-radius: 50%;
		background: var(--crew, var(--ink-muted));
		opacity: 1;
		animation: dot 1.2s ease-in-out infinite;
	}
	.dots i:nth-child(2) {
		animation-delay: 0.15s;
	}
	.dots i:nth-child(3) {
		animation-delay: 0.3s;
	}
	@keyframes dot {
		30% {
			opacity: 1;
			transform: translateY(-0.2em);
		}
	}
	.crew {
		display: flex;
		align-items: flex-start;
		gap: 0.7em;
	}
	.face {
		flex: none;
		filter: drop-shadow(0 0.4em 0.8em rgb(0 0 0 / 0.35));
	}
	.face.hop {
		animation: hop 900ms cubic-bezier(0.3, 1.6, 0.5, 1) both;
	}
	@keyframes hop {
		40% {
			transform: translateY(-0.6em) scale(1.04, 0.96);
		}
	}
	.card {
		flex: 1;
		min-width: 0;
		padding: 0.8em 0.95em;
		border-radius: 1em;
		background: rgb(10 16 40 / 0.72);
		border: 1px solid rgb(255 255 255 / 0.1);
		backdrop-filter: blur(8px);
		font-size: 0.82em;
	}
	.card-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.45em;
	}
	.status {
		font-size: 0.85em;
	}
	.status.ok {
		color: var(--ok);
	}
	.status.warn {
		color: var(--warn);
	}
	.shimmer {
		background: linear-gradient(90deg, #8f9bd6 0%, #fff 50%, #8f9bd6 100%);
		background-size: 200% 100%;
		-webkit-background-clip: text;
		background-clip: text;
		color: transparent;
		animation: shimmer 1.8s linear infinite;
	}
	@keyframes shimmer {
		to {
			background-position: -200% 0;
		}
	}
	.event {
		display: inline-flex;
		align-items: center;
		gap: 0.4em;
		color: var(--ink-muted);
		margin-bottom: 0.4em;
	}
	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3em;
	}
	.steps li {
		display: flex;
		align-items: center;
		gap: 0.5em;
		color: var(--ink-soft);
	}
	.tick {
		display: inline-grid;
		place-items: center;
		width: 1.3em;
		height: 1.3em;
		border-radius: 50%;
		background: rgb(52 211 153 / 0.18);
		color: var(--ok);
		flex: none;
	}

	.ask {
		margin-top: 0.7em;
		padding: 0.7em 0.8em;
		border-radius: 0.75em;
		background: rgb(251 191 36 / 0.08);
		border: 1px solid rgb(251 191 36 / 0.45);
		display: flex;
		flex-direction: column;
		gap: 0.35em;
	}
	.ask code {
		font-family: ui-monospace, "SF Mono", Menlo, monospace;
		font-size: 0.95em;
		color: var(--ink-strong);
	}
	.ask-btns {
		position: relative;
		display: flex;
		gap: 0.45em;
		justify-content: flex-end;
	}
	.btn {
		padding: 0.3em 0.85em;
		border-radius: 999px;
		font-size: 0.9em;
		background: rgb(255 255 255 / 0.08);
		border: 1px solid rgb(255 255 255 / 0.14);
	}
	.btn.primary {
		background: #3d5bff;
		border-color: #3d5bff;
		color: var(--ink-strong);
	}
	.allowed {
		margin-top: 0.7em;
		display: inline-flex;
		align-items: center;
		gap: 0.45em;
		color: var(--ok);
	}

	.phone {
		position: absolute;
		right: 5%;
		bottom: 4%;
		width: 11em;
		padding: 0.6em;
		border-radius: 1.4em;
		background: #0a0f24;
		border: 2px solid rgb(255 255 255 / 0.14);
		box-shadow: 0 1em 2.5em rgb(0 0 0 / 0.45);
		opacity: 0;
		transform: translateY(1.5em) rotate(4deg);
		transition:
			opacity 450ms ease,
			transform 550ms cubic-bezier(0.2, 1.3, 0.4, 1);
	}
	.phone.in {
		opacity: 1;
		transform: none;
	}
	.notif {
		padding: 0.6em;
		border-radius: 0.9em;
		background: rgb(255 255 255 / 0.07);
		display: flex;
		flex-direction: column;
		gap: 0.25em;
		font-size: 0.72em;
	}
	.notif > span {
		color: var(--ink-muted);
		font-family: ui-monospace, "SF Mono", Menlo, monospace;
	}
	.ring {
		position: absolute;
		right: 0.2em;
		top: 50%;
		width: 2.4em;
		height: 2.4em;
		margin-top: -1.2em;
		border-radius: 50%;
		border: 2px solid #fff;
		pointer-events: none;
	}

	.composer {
		position: absolute;
		left: 50%;
		bottom: 21%;
		translate: -50% 0;
		width: min(26em, 80%);
		padding: 0.75em 1.1em;
		border-radius: 999px;
		font-size: 0.8em;
		background: rgb(10 16 40 / 0.8);
		border: 1px solid rgb(255 255 255 / 0.14);
		backdrop-filter: blur(8px);
		white-space: nowrap;
		overflow: hidden;
		transition:
			opacity 400ms ease,
			transform 400ms ease;
	}
	.composer.gone {
		opacity: 0;
		transform: translateY(0.8em);
	}
	.caret {
		display: inline-block;
		width: 2px;
		height: 1.1em;
		margin-left: 2px;
		vertical-align: -0.2em;
		background: #fff;
		animation: blink 1s steps(1) infinite;
	}
	@keyframes blink {
		50% {
			opacity: 0;
		}
	}

	.pop {
		animation: pop 900ms cubic-bezier(0.16, 1, 0.3, 1) both;
	}
	@keyframes pop {
		from {
			opacity: 0;
			filter: blur(6px);
			transform: translateY(1em) scale(0.97);
		}
	}
	.step-in {
		animation: step-in 800ms cubic-bezier(0.16, 1, 0.3, 1) both;
	}
	@keyframes step-in {
		from {
			opacity: 0;
			filter: blur(4px);
			transform: translateY(0.4em);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.pop,
		.step-in,
		.face.hop,
		.shimmer,
		.dot.live,
		.caret,
		.dots i,
		.mark > *,
		.mark .dots i {
			animation: none;
		}
		.mark .mini {
			opacity: 0;
		}
	}
</style>
