<script lang="ts">
	import { fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import BotFace from "#lib/bots/BotFace.svelte";
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
	import { Server, MessageSquare, BellRing, Copy, Check, ChevronLeft } from "@lucide/svelte";
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

	// Floating faces for intro (fixed seeds for determinism)
	const INTRO_FACES = [
		{ seed: "face-a", size: 40, x: 18, y: 22, phase: 0 },
		{ seed: "face-b", size: 32, x: 72, y: 15, phase: 0.4 },
		{ seed: "face-c", size: 48, x: 12, y: 58, phase: 0.7 },
		{ seed: "face-d", size: 28, x: 80, y: 62, phase: 1.1 },
		{ seed: "face-e", size: 36, x: 50, y: 12, phase: 1.6 },
	];

	// ─── computed ─────────────────────────────────────────────────────────────
	const integrations = $derived(
		agents.list.filter((a) => ["claude", "opencode", "codex", "omp"].includes(a.id)),
	);

	// ─── actions ──────────────────────────────────────────────────────────────
	function advance() {
		const next = nextStep({ step, hasHost });
		if (next === "done") finish();
		else step = next;
	}

	function back() {
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
			<!-- Floating faces -->
			{#each INTRO_FACES as f}
				<div
					class="float-face"
					class:reduced={reducedMotion}
					style:left="{f.x}%"
					style:top="{f.y}%"
					style:--phase="{f.phase}s"
					aria-hidden="true"
				>
					<BotFace seed={f.seed} state="idle" size={f.size} />
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
			<div class="how-content">
				<div class="how-icon">
					{#if step === "how1"}
						<Server size={64} strokeWidth={1.25} />
					{:else if step === "how2"}
						<MessageSquare size={64} strokeWidth={1.25} />
					{:else}
						<BellRing size={64} strokeWidth={1.25} />
					{/if}
				</div>
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

			<!-- Pager dots -->
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
		z-index: 200;
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
		animation: bob 4s ease-in-out infinite;
		animation-delay: var(--phase, 0s);
	}
	.float-face.reduced {
		animation: none;
	}
	@keyframes bob {
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-10px);
		}
	}

	.intro-center {
		position: relative;
		z-index: 1;
		text-align: center;
		max-width: 480px;
		padding: 0 24px;
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
	.step-how {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 24px;
		max-width: 480px;
		padding: 0 24px;
		text-align: center;
	}

	.how-icon {
		color: var(--ca-text-primary);
		opacity: 0.85;
	}

	.how-title {
		font-size: 24px;
		font-weight: 700;
		color: var(--ca-text-primary);
		margin: 0;
	}

	.how-body {
		font-size: 15px;
		color: var(--ca-text-secondary);
		line-height: 1.6;
		margin: 0;
	}

	.pager {
		display: flex;
		gap: 6px;
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
