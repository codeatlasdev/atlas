<script lang="ts">
	import { fly } from "svelte/transition";
	import { cubicOut, cubicIn } from "svelte/easing";
	import type { TransitionConfig } from "svelte/transition";
	import BotFace from "#lib/bots/BotFace.svelte";
	import BotActor from "#lib/bots/BotActor.svelte";
	import CoffeeCup from "#lib/bots/props/CoffeeCup.svelte";
	import SceneServer from "#lib/bots/SceneServer.svelte";
	import SceneChat from "#lib/bots/SceneChat.svelte";
	import SceneNotif from "#lib/bots/SceneNotif.svelte";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { randomLook } from "#lib/bots/look.ts";
	import { dur, reducedMotion } from "#lib/bots/motion.ts";
	import {
		INITIAL_STEP,
		PAGER_STEPS,
		nextStep,
		prevStep,
		pagerIndex,
		isEngineReady,
		type OnboardingStep,
	} from "#lib/bots/botsOnboarding.ts";
	import { Copy, Check, ChevronLeft } from "@lucide/svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";

	let { ondone }: { ondone: () => void } = $props();

	// ─── state ────────────────────────────────────────────────────────────────
	let step = $state<OnboardingStep>(INITIAL_STEP);
	let hasHost = $derived(bots.hosts.some((h) => h.key !== ""));

	// connect step
	let sshTarget = $state("");
	let connecting = $state(false);
	let connectError = $state("");
	let connected = $state(false);

	// engine step
	let selectedEngine = $state("");

	// slide direction: 1 = forward (right-to-left), -1 = backward
	let direction = $state(1);

	// ─── intro faces with per-character actions ────────────────────────────────
	// face-a waves at the cursor (top-left), face-c is asleep (bottom-left),
	// face-d sips coffee (bottom-right), the rest drift idly.
	const INTRO_FACES = [
		{
			seed: "face-a",
			size: 72,
			x: 8,
			y: 18,
			phase: 0,
			action: "wave" as const,
			state: "idle" as const,
			hasCoffee: false,
		},
		{
			seed: "face-b",
			size: 56,
			x: 76,
			y: 10,
			phase: 0.5,
			action: "idle" as const,
			state: "idle" as const,
			hasCoffee: false,
		},
		{
			seed: "face-c",
			size: 88,
			x: 5,
			y: 62,
			phase: 0.9,
			action: "idle" as const,
			state: "offline" as const,
			hasCoffee: false,
		},
		{
			seed: "face-d",
			size: 64,
			x: 80,
			y: 65,
			phase: 1.4,
			action: "sip" as const,
			state: "idle" as const,
			hasCoffee: true,
		},
		{
			seed: "face-e",
			size: 48,
			x: 44,
			y: 6,
			phase: 1.9,
			action: "idle" as const,
			state: "idle" as const,
			hasCoffee: false,
		},
	];

	// ─── computed ─────────────────────────────────────────────────────────────
	const integrations = $derived(
		agents.list.filter((a) => ["claude", "opencode", "codex", "omp"].includes(a.id)),
	);

	// Connect step: bot avatar state reflects connection status
	const connectBotState = $derived(
		connected
			? ("needs_you" as const)
			: connecting
				? ("working" as const)
				: connectError
					? ("needs_you" as const)
					: ("offline" as const),
	);

	// ─── parallax slide transitions ───────────────────────────────────────────
	// Art moves ~half as far as text so it feels deeper / layered.
	function artIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : dur.slow,
			easing: cubicOut,
			css: (t) => `opacity:${t};transform:translateX(${(1 - t) * d * 56}px)`,
		};
	}
	function artOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : dur.base,
			easing: cubicIn,
			css: (t) => `opacity:${t};transform:translateX(${(1 - t) * -d * 56}px)`,
		};
	}
	function textIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : dur.slow,
			easing: cubicOut,
			css: (t) => `opacity:${t};transform:translateX(${(1 - t) * d * 112}px)`,
		};
	}
	function textOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : dur.base,
			easing: cubicIn,
			css: (t) => `opacity:${t};transform:translateX(${(1 - t) * -d * 112}px)`,
		};
	}

	// ─── actions ──────────────────────────────────────────────────────────────
	function advance() {
		direction = 1;
		const next = nextStep({ step, hasHost });
		if (next === "done") finish();
		else step = next;
	}

	function back() {
		direction = -1;
		const prev = prevStep({ step, hasHost });
		if (prev) step = prev;
	}

	function finish() {
		try {
			localStorage.setItem("bots:onboarding:seen", "1");
		} catch {
			/* private browsing */
		}
		ondone();
	}

	async function connectHost() {
		if (!sshTarget.trim()) return;
		connecting = true;
		connectError = "";
		try {
			await bots.addHost(sshTarget.trim(), sshTarget.trim());
			connected = true;
			setTimeout(advance, 800);
		} catch (e) {
			connectError = errText(e);
		} finally {
			connecting = false;
		}
	}

	async function createBot() {
		if (!selectedEngine) return;
		const hostKey = bots.hosts[0]?.key ?? "";
		try {
			const id = await bots.createBot(hostKey, {
				name: "Atlas Bot",
				role: "",
				agent: selectedEngine,
				approval: "auto",
				look: randomLook(),
			});
			await bots.select(hostKey, id);
			finish();
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function copyInstall() {
		await navigator.clipboard.writeText(`atlas bot install ${sshTarget || "user@host"}`).catch(() => {});
		toast.success("Copied");
	}
</script>

<!-- Full-screen overlay -->
<div class="onboarding-overlay" role="dialog" aria-modal="true" aria-label="Atlas Bot setup">
	<!-- Skip always visible -->
	{#if step !== "done"}
		<button class="skip-btn" onclick={finish}>Skip</button>
	{/if}

	<!-- ─── Intro ──────────────────────────────────────────────────────────── -->
	{#if step === "intro"}
		<div class="step step-intro" in:fly={{ y: 24, duration: reducedMotion ? 0 : dur.slow, easing: cubicOut }}>
			<!-- Floating bot characters with actions -->
			{#each INTRO_FACES as f}
				<div
					class="float-face"
					class:reduced={reducedMotion}
					style:left="{f.x}%"
					style:top="{f.y}%"
					style:--phase="{f.phase}s"
					aria-hidden="true"
				>
					{#if f.hasCoffee}
						<!-- Sipping bot: face + coffee cup side by side -->
						<div class="face-with-prop">
							<BotActor seed={f.seed} state={f.state} size={f.size} action={f.action} />
							<div class="prop-coffee" style:color="var(--ca-text-secondary)">
								<CoffeeCup size={Math.round(f.size * 0.45)} />
							</div>
						</div>
					{:else}
						<BotActor seed={f.seed} state={f.state} size={f.size} action={f.action} />
					{/if}
				</div>
			{/each}

			<div class="intro-center">
				<h1 class="intro-title">Meet Atlas Bot</h1>
				<p class="intro-sub">
					Agents that work 24/7 on your own server. Talk to them from your desktop, your phone or Telegram.
				</p>
				<button class="pill-btn" onclick={advance}>Continue</button>
			</div>
		</div>

		<!-- ─── How it works slides ───────────────────────────────────────────── -->
	{:else if step === "how1" || step === "how2" || step === "how3"}
		{@const slideIdx = pagerIndex(step)}
		<div class="step step-how" in:fly={{ y: 24, duration: reducedMotion ? 0 : dur.base, easing: cubicOut }}>
			<!-- Art zone: scene component with parallax in/out on step change -->
			{#key step}
				<div class="how-art" in:artIn={{ d: direction }} out:artOut={{ d: direction }}>
					{#if step === "how1"}
						<SceneServer />
					{:else if step === "how2"}
						<SceneChat />
					{:else}
						<SceneNotif />
					{/if}
				</div>

				<!-- Text zone: slides faster than art for parallax depth -->
				<div class="how-text" in:textIn={{ d: direction }} out:textOut={{ d: direction }}>
					<h2 class="how-title">
						{#if step === "how1"}It runs on your server
						{:else if step === "how2"}You chat, it works
						{:else}It calls you when it needs you{/if}
					</h2>
					<p class="how-body">
						{#if step === "how1"}Your server never sleeps. Your Mac can be off — the bot keeps going.
						{:else if step === "how2"}Continuous conversation. The bot remembers what it did last time.
						{:else}Approvals and notifications on your phone, any time a bot needs a decision.{/if}
					</p>
				</div>
			{/key}

			<!-- Pager dots — outside key block, no transition -->
			<div class="pager" aria-label="Step {slideIdx + 1} of 3">
				{#each PAGER_STEPS as _, i}
					<span class="dot" class:active={i === slideIdx}></span>
				{/each}
			</div>

			<!-- Nav -->
			<div class="how-nav">
				<button class="text-btn" onclick={back}>
					<ChevronLeft size={14} />Back
				</button>
				<button class="pill-btn" onclick={advance}>Continue</button>
			</div>
		</div>

		<!-- ─── Connect ───────────────────────────────────────────────────────── -->
	{:else if step === "connect"}
		<div
			class="step step-connect"
			in:fly={{ y: 24, duration: reducedMotion ? 0 : dur.base, easing: cubicOut }}
		>
			<!-- Reactive bot avatar: sleepy while idle, works while connecting, hops on success -->
			<div class="step-avatar" aria-hidden="true">
				<BotFace seed="onboarding-connect" state={connectBotState} size={72} />
			</div>

			{#if connected}
				<div class="connect-success">
					<span class="check-icon"><Check size={32} /></span>
					<p>Bot server connected.</p>
				</div>
			{:else}
				<h2 class="step-title">Connect a bot server</h2>
				<p class="step-sub">Run this on your Linux server first:</p>
				<div class="code-block">
					<code>atlas bot install {sshTarget || "user@host"}</code>
					<button class="copy-btn" onclick={copyInstall} aria-label="Copy command">
						<Copy size={12} />
					</button>
				</div>
				<input class="ssh-input" bind:value={sshTarget} placeholder="user@host" aria-label="SSH target" />
				{#if connectError}
					<p class="connect-error" role="alert">{connectError}</p>
				{/if}
				<div class="connect-nav">
					<button class="text-btn" onclick={back}><ChevronLeft size={14} />Back</button>
					<button class="pill-btn" onclick={connectHost} disabled={connecting || !sshTarget.trim()}>
						{connecting ? "Connecting…" : "Connect"}
					</button>
				</div>
				<button class="text-btn center-link" onclick={advance}>Skip for now</button>
			{/if}
		</div>

		<!-- ─── Engine ────────────────────────────────────────────────────────── -->
	{:else if step === "engine"}
		<div
			class="step step-engine"
			in:fly={{ y: 24, duration: reducedMotion ? 0 : dur.base, easing: cubicOut }}
		>
			<!-- Bot avatar: idle until an engine is selected, then working -->
			<div class="step-avatar" aria-hidden="true">
				<BotFace seed="onboarding-engine" state={selectedEngine ? "working" : "idle"} size={72} />
			</div>

			<h2 class="step-title">Choose an engine</h2>
			<p class="step-sub">Select the AI agent your bot will use.</p>
			<div class="engine-grid">
				{#each integrations as agent}
					{@const ready = isEngineReady(agent)}
					<button
						class="engine-card"
						class:selected={selectedEngine === agent.id}
						class:disabled={!ready}
						disabled={!ready}
						onclick={() => ready && (selectedEngine = agent.id)}
						aria-pressed={selectedEngine === agent.id}
					>
						<span class="engine-label">{agent.label}</span>
						{#if !ready}
							<span class="engine-hint">atlas bot login server {agent.id}</span>
						{/if}
					</button>
				{/each}
				{#if integrations.length === 0}
					<p class="engine-empty">No agents found. Install one with <code>atlas setup</code>.</p>
				{/if}
			</div>
			<div class="engine-nav">
				<button class="text-btn" onclick={back}><ChevronLeft size={14} />Back</button>
				<button class="pill-btn" onclick={createBot} disabled={!selectedEngine}> Create bot </button>
			</div>
			<button class="text-btn center-link" onclick={finish}>Skip for now</button>
		</div>
	{/if}
</div>

<style>
	.onboarding-overlay {
		position: fixed;
		inset: 0;
		z-index: 1600;
		background: var(--ca-surface-base);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.skip-btn {
		position: absolute;
		top: 20px;
		right: 20px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 13px;
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 6px;
		z-index: 1;
	}
	.skip-btn:hover {
		background: var(--ca-vibrancy-fill);
	}

	/* ─── Intro ──────────────────────────────────────────────── */
	.step-intro {
		position: relative;
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.float-face {
		position: absolute;
		animation: bob 5s ease-in-out infinite;
		animation-delay: var(--phase, 0s);
	}
	.float-face.reduced {
		animation: none;
	}
	@keyframes bob {
		0%,
		100% {
			transform: translateY(0px) rotate(-2deg);
		}
		50% {
			transform: translateY(-12px) rotate(2deg);
		}
	}

	/* coffee cup sits beside the sipping bot */
	.face-with-prop {
		display: flex;
		align-items: flex-end;
		gap: 4px;
	}
	.prop-coffee {
		margin-bottom: 6px;
	}

	.intro-center {
		position: relative;
		z-index: 1;
		text-align: center;
		max-width: 480px;
		padding: 0 24px;
		display: flex;
		flex-direction: column;
		align-items: center;
	}

	.intro-title {
		font-size: 32px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0 0 12px;
	}

	.intro-sub {
		font-size: 15px;
		color: var(--ca-text-secondary);
		line-height: 1.6;
		margin: 0 0 32px;
	}

	/* ─── How slides ─────────────────────────────────────────── */
	/* Full-screen flex column: art fills upper half, text+nav anchors bottom */
	.step-how {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		overflow: hidden;
	}

	.how-art {
		flex: 1 1 0;
		min-height: 200px;
		max-height: 360px;
		width: 100%;
		max-width: 720px;
		padding: 16px 16px 0;
		box-sizing: border-box;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		position: relative;
	}

	.how-text {
		flex: 0 0 auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		padding: 20px 24px 4px;
		max-width: 520px;
		width: 100%;
		text-align: center;
	}

	.how-title {
		font-size: 22px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0;
	}

	.how-body {
		font-size: 14px;
		color: var(--ca-text-secondary);
		line-height: 1.6;
		margin: 0;
	}

	.pager {
		display: flex;
		gap: 6px;
		padding: 12px 0 4px;
	}

	.pager .dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		opacity: 0.35;
		transition:
			opacity 200ms,
			transform 200ms;
	}

	.pager .dot.active {
		opacity: 1;
		transform: scale(1.25);
		background: var(--ca-accent, #4f8ef7);
	}

	.how-nav {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 0 20px;
	}

	/* ─── Connect + Engine ───────────────────────────────────── */
	.step-connect,
	.step-engine {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		max-width: 460px;
		width: 100%;
		padding: 0 24px;
		text-align: center;
	}

	/* Reactive bot avatar at top of form steps */
	.step-avatar {
		margin-bottom: 4px;
	}

	.step-title {
		font-size: 22px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0;
	}

	.step-sub {
		font-size: 14px;
		color: var(--ca-text-secondary);
		margin: 0;
	}

	.code-block {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--ca-surface-sunken);
		border-radius: 8px;
		padding: 10px 14px;
		width: 100%;
		font-family: monospace;
		font-size: 12px;
		color: var(--ca-text-primary);
		text-align: left;
		box-sizing: border-box;
	}

	.code-block code {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.copy-btn {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--ca-text-secondary);
		padding: 2px;
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.copy-btn:hover {
		color: var(--ca-text-primary);
	}

	.ssh-input {
		width: 100%;
		background: var(--ca-surface-sunken);
		border: 1px solid var(--ca-outline-subtle);
		border-radius: 8px;
		padding: 10px 14px;
		font-size: 13px;
		color: var(--ca-text-primary);
		box-sizing: border-box;
		outline: none;
	}
	.ssh-input:focus {
		border-color: var(--ca-accent, #4f8ef7);
	}

	.connect-error {
		font-size: 12px;
		color: var(--ca-status-error, #ef4444);
		margin: 0;
	}

	.connect-nav {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.center-link {
		margin-top: -8px;
	}

	.connect-success {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		color: var(--ca-text-primary);
	}

	.check-icon {
		color: var(--ca-semantic-success, #22c55e);
	}

	/* ─── Engine ─────────────────────────────────────────────── */
	.engine-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
		width: 100%;
	}

	.engine-card {
		background: var(--ca-surface-sunken);
		border: 1.5px solid var(--ca-outline-subtle);
		border-radius: 10px;
		padding: 16px 12px;
		cursor: pointer;
		text-align: left;
		display: flex;
		flex-direction: column;
		gap: 4px;
		transition:
			border-color 150ms,
			background 150ms;
	}
	.engine-card:hover:not(.disabled) {
		border-color: var(--ca-accent, #4f8ef7);
	}
	.engine-card.selected {
		border-color: var(--ca-accent, #4f8ef7);
		background: color-mix(in srgb, var(--ca-accent, #4f8ef7) 12%, transparent);
	}
	.engine-card.disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.engine-label {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}

	.engine-hint {
		font-size: 10px;
		color: var(--ca-text-secondary);
		font-family: monospace;
	}

	.engine-empty {
		grid-column: 1 / -1;
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
	}

	.engine-nav {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	/* ─── Shared buttons ─────────────────────────────────────── */
	.pill-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		border: none;
		border-radius: 999px;
		padding: 10px 28px;
		font-size: 14px;
		font-weight: 600;
		cursor: pointer;
		transition: opacity 150ms;
		width: min(440px, 90%);
	}
	.pill-btn:hover {
		opacity: 0.85;
	}
	.pill-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.text-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 13px;
		cursor: pointer;
		padding: 6px 10px;
		border-radius: 6px;
	}
	.text-btn:hover {
		color: var(--ca-text-primary);
		background: var(--ca-vibrancy-fill);
	}
</style>
