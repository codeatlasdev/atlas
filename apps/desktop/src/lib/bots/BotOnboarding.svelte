<script lang="ts">
	import { cubicOut, cubicIn } from "svelte/easing";
	import type { TransitionConfig } from "svelte/transition";
	import BotArtIntro from "#lib/bots/BotArtIntro.svelte";
	import BotArtServer from "#lib/bots/BotArtServer.svelte";
	import BotArtChat from "#lib/bots/BotArtChat.svelte";
	import BotArtApproval from "#lib/bots/BotArtApproval.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import BotListRow from "#lib/bots/BotListRow.svelte";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { randomLook, normalizeLook } from "#lib/bots/look.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { agentLogo } from "#lib/agentLogos.ts";
	import {
		INITIAL_STEP,
		INFO_STEPS,
		nextStep,
		prevStep,
		infoStepIndex,
		stepProgress,
		isEngineReady,
		type OnboardingStep,
	} from "#lib/bots/botsOnboarding.ts";
	import { Copy, Check, ChevronLeft } from "@lucide/svelte";
	import { toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";

	let { ondone }: { ondone: () => void } = $props();

	// ─── step state ───────────────────────────────────────────────────────────
	let step = $state<OnboardingStep>(INITIAL_STEP);
	let direction = $state(1);
	let hasHost = $derived(bots.hosts.some((h) => h.key !== ""));

	// ─── connect step ─────────────────────────────────────────────────────────
	let sshTarget = $state("");
	let connecting = $state(false);
	let connectError = $state("");
	let connected = $state(false);
	let fieldShake = $state(false);

	// ─── engine step ──────────────────────────────────────────────────────────
	let selectedEngine = $state("");

	// ─── ready step ───────────────────────────────────────────────────────────
	let creatingBot = $state(false);
	const previewLook = randomLook();

	// ─── derived ──────────────────────────────────────────────────────────────
	const integrations = $derived(
		agents.list.filter((a) => ["claude", "opencode", "codex", "omp"].includes(a.id)),
	);

	const signInHost = $derived(sshTarget.trim() || "user@host");

	const progress = $derived(stepProgress({ step, hasHost }));

	const infoIdx = $derived(infoStepIndex(step));
	const isInfoStep = $derived(infoIdx >= 0);

	const mockBot = $derived({
		id: "onb-preview",
		name: "My First Bot",
		agent: selectedEngine || "claude",
		state: "idle" as const,
		role: "",
		approval: "auto" as const,
		queued: 0,
		last: undefined,
		look: previewLook,
	});

	// ─── engine auth hints ────────────────────────────────────────────────────
	const ENGINE_HINTS: Record<string, string> = {
		claude: "Your Claude subscription",
		codex: "Your ChatGPT subscription",
		omp: "API key or OpenRouter",
		opencode: "Open source",
	};

	// ─── per-step content ─────────────────────────────────────────────────────
	type StepMeta = {
		label?: string;
		headline: string;
		body: string;
		bullets?: string[];
		glow: string;
	};

	const STEP_META: Record<string, StepMeta> = {
		intro: {
			label: "1 of 4",
			headline: "Agents that keep working when you don't",
			body: "Atlas Bot runs 24/7 on your own server. Talk to it from your desktop, phone or Telegram.",
			bullets: [
				"Runs on your server, not CodeAtlas cloud",
				"Any AI engine on your own subscription",
				"Desktop, phone and Telegram",
			],
			glow: "rgba(61, 91, 255, 0.10)",
		},
		how1: {
			label: "2 of 4",
			headline: "Always on, even when your Mac is off",
			body: "Your bot runs as a Linux service you control. Your laptop can sleep — the bot keeps going.",
			bullets: [
				"Never idle: systemd keeps it alive",
				"Secure SSH tunnel back to your desktop",
				"No CodeAtlas cloud in the loop",
			],
			glow: "rgba(34, 197, 94, 0.08)",
		},
		how2: {
			label: "3 of 4",
			headline: "Send a task, get back a result",
			body: "Type a task in plain language. The bot picks it up, works through it step by step, and shows you everything.",
			bullets: undefined,
			glow: "rgba(61, 91, 255, 0.10)",
		},
		how3: {
			label: "4 of 4",
			headline: "You stay in control",
			body: "When the bot needs to run something sensitive, it pauses and waits for your OK — on desktop, phone or Telegram.",
			bullets: [
				"Approval requests on all your devices",
				"30 minutes to respond, then it stops",
				"Full audit trail in the chat thread",
			],
			glow: "rgba(245, 158, 11, 0.12)",
		},
		connect: {
			headline: "Connect a bot server",
			body: "Run this command on your Linux server, then enter its address below.",
			glow: "rgba(61, 91, 255, 0.08)",
		},
		engine: {
			headline: "Choose an engine",
			body: "Select the AI your bot will use. Each engine runs on your own subscription or API key.",
			glow: "rgba(61, 91, 255, 0.10)",
		},
		ready: {
			headline: "Your first bot is ready",
			body: "Open a chat to send your first task. You can add more bots and customise them from the workspace.",
			glow: "rgba(34, 197, 94, 0.12)",
		},
	};

	const meta = $derived(STEP_META[step]);

	// ─── parallax transitions ─────────────────────────────────────────────────

	function textIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 380,
			easing: cubicOut,
			// Only translateX — children stagger opacity via CSS animation
			css: (t) => `transform: translateX(${(1 - t) * -d * 64}px)`,
		};
	}

	function textOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 200,
			easing: cubicIn,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * d * 64}px)`,
		};
	}

	function artIn(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 400,
			easing: cubicOut,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * d * 220}px) scale(${0.92 + 0.08 * t})`,
		};
	}

	function artOut(_node: Element, { d }: { d: number }): TransitionConfig {
		return {
			duration: reducedMotion ? 0 : 200,
			easing: cubicIn,
			css: (t) => `opacity: ${t}; transform: translateX(${(1 - t) * -d * 220}px) scale(${0.92 + 0.08 * t})`,
		};
	}

	// ─── navigation ───────────────────────────────────────────────────────────

	function advance() {
		direction = 1;
		const next = nextStep({ step, hasHost });
		if (next === "done") finish();
		else step = next as OnboardingStep;
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

	// ─── connect step actions ─────────────────────────────────────────────────

	async function connectHost() {
		const target = sshTarget.trim();
		if (!target) return;
		connecting = true;
		connectError = "";
		try {
			await bots.addHost(target, target);
			connected = true;
			setTimeout(advance, 900);
		} catch (e) {
			connectError = errText(e);
			fieldShake = true;
			setTimeout(() => (fieldShake = false), 500);
		} finally {
			connecting = false;
		}
	}

	async function copyInstall() {
		await navigator.clipboard
			.writeText(`atlas bot install ${sshTarget.trim() || "user@host"}`)
			.catch(() => {});
		toast.success("Copied");
	}

	async function copySignIn(agentId: string) {
		await navigator.clipboard.writeText(`atlas bot login ${signInHost} ${agentId}`).catch(() => {});
		toast.success("Copied");
	}

	// ─── engine / ready step actions ─────────────────────────────────────────

	function selectEngine(id: string) {
		selectedEngine = id;
		// Automatically advance to ready after a short hold so selection is visible
		setTimeout(advance, 560);
	}

	async function openChat() {
		if (creatingBot) return;
		const hostKey = bots.hosts[0]?.key ?? "";
		creatingBot = true;
		try {
			if (selectedEngine) {
				const id = await bots.createBot(hostKey, {
					name: "My First Bot",
					role: "",
					agent: selectedEngine,
					approval: "auto",
					look: previewLook,
				});
				await bots.select(hostKey, id);
			}
			finish();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			creatingBot = false;
		}
	}
</script>

<!-- ─── Full-window overlay ─────────────────────────────────────────────── -->
<div
	class="overlay"
	role="dialog"
	aria-modal="true"
	aria-label="Atlas Bot setup"
	data-testid="bots-onboarding"
>
	<!-- Progress bar + Skip -->
	<div class="top-bar">
		<div class="progress-track" role="progressbar" aria-valuenow={Math.round(progress * 100)}>
			<div class="progress-fill" style:width="{progress * 100}%"></div>
		</div>
		<button class="skip-btn" onclick={finish}>Skip</button>
	</div>

	<!-- Two-column body -->
	<div class="body">
		<!-- ─── LEFT: text content ─────────────────────────────────────────── -->
		<div class="left-panel">
			{#key step}
				<div class="text-block" in:textIn={{ d: direction }} out:textOut={{ d: direction }}>
					<!-- Info step label "N of 4" -->
					{#if isInfoStep}
						<span class="step-label" style:--stagger={0}>{meta?.label}</span>
					{:else if step === "connect" || step === "engine"}
						<span class="step-label setup" style:--stagger={0}>Setup</span>
					{/if}

					<!-- Headline -->
					<h1 class="headline" style:--stagger={1}>{meta?.headline ?? ""}</h1>

					<!-- Body text -->
					<p class="body-text" style:--stagger={2}>{meta?.body ?? ""}</p>

					<!-- Bullet points (info steps only) -->
					{#if meta?.bullets?.length}
						<ul class="bullets" style:--stagger={3}>
							{#each meta.bullets as bullet}
								<li>{bullet}</li>
							{/each}
						</ul>
					{/if}

					<!-- ── Connect step form ──────────────────────────────────── -->
					{#if step === "connect"}
						<div class="form-block" style:--stagger={3}>
							<!-- Install command code block -->
							<div class="code-block">
								<code>atlas bot install {sshTarget.trim() || "user@host"}</code>
								<button class="copy-btn" onclick={copyInstall} aria-label="Copy install command">
									<Copy size={13} />
								</button>
							</div>

							<!-- SSH target input -->
							<label class="field-label" for="ssh-target">Server address</label>
							<div class="field-wrap" class:shake={fieldShake}>
								<input
									id="ssh-target"
									class="ssh-input"
									bind:value={sshTarget}
									placeholder="user@host"
									aria-label="SSH target"
									disabled={connecting || connected}
									onkeydown={(e) => e.key === "Enter" && connectHost()}
								/>
							</div>

							{#if connectError}
								<p class="field-error" role="alert">{connectError}</p>
							{/if}

							{#if connected}
								<div class="connect-success">
									<Check size={15} />
									<span>Connected — continuing…</span>
								</div>
							{/if}
						</div>
					{/if}

					<!-- ── Engine step form ───────────────────────────────────── -->
					{#if step === "engine"}
						<div class="engine-list" style:--stagger={3}>
							{#each integrations as agent (agent.id)}
								{@const ready = isEngineReady(agent)}
								{@const logo = agentLogo(agent.id)}
								<button
									class="engine-card"
									class:selected={selectedEngine === agent.id}
									onclick={() => ready && selectEngine(agent.id)}
									disabled={!ready}
									aria-pressed={selectedEngine === agent.id}
								>
									<div class="engine-logo">
										{#if logo}
											{@html logo.dark ?? logo.light}
										{:else}
											<div class="engine-logo-fallback">{agent.label[0]}</div>
										{/if}
									</div>
									<div class="engine-info">
										<span class="engine-name">{agent.label}</span>
										<span class="engine-auth">{ENGINE_HINTS[agent.id] ?? "Your subscription"}</span>
									</div>
									{#if ready}
										<span class="engine-status ready">Ready</span>
									{:else}
										<span
											class="engine-status sign-in"
											role="button"
											tabindex={ready ? -1 : 0}
											onclick={(e) => {
												e.stopPropagation();
												copySignIn(agent.id);
											}}
											onkeydown={(e) => e.key === "Enter" && (e.stopPropagation(), copySignIn(agent.id))}
											aria-label="Copy sign-in command for {agent.label}"
										>
											<Copy size={10} />
											<span>atlas bot login {signInHost} {agent.id}</span>
										</span>
									{/if}
								</button>
							{/each}

							{#if integrations.length === 0}
								<p class="engine-empty">
									No agents found. Install one with <code>atlas setup</code>.
								</p>
							{/if}
						</div>
					{/if}

					<!-- ─── Actions row ────────────────────────────────────────── -->
					<div class="actions" style:--stagger={meta?.bullets?.length ? 4 : 3}>
						<!-- Back button (not on intro) -->
						{#if step !== "intro" && step !== "ready"}
							<button class="back-btn" onclick={back}>
								<ChevronLeft size={15} />Back
							</button>
						{/if}

						{#if step === "intro" || isInfoStep}
							<button class="primary-btn" onclick={advance}>Continue</button>
						{:else if step === "connect"}
							<div class="action-group">
								<button
									class="primary-btn"
									onclick={connectHost}
									disabled={connecting || !sshTarget.trim() || connected}
								>
									{connecting ? "Connecting…" : "Connect"}
								</button>
								<button class="text-link" onclick={advance}>Skip for now</button>
							</div>
						{:else if step === "engine"}
							<div class="action-group">
								<button class="text-link" onclick={finish}>Skip for now</button>
							</div>
						{:else if step === "ready"}
							<button class="primary-btn" onclick={openChat} disabled={creatingBot}>
								{creatingBot ? "Opening…" : "Open chat"}
							</button>
							<button class="back-btn" onclick={back}>
								<ChevronLeft size={15} />Back
							</button>
						{/if}
					</div>
				</div>
			{/key}
		</div>

		<!-- ─── RIGHT: illustration ──────────────────────────────────────── -->
		<div class="right-panel">
			{#key step}
				<div
					class="art-zone"
					style:--art-glow={meta?.glow ?? "transparent"}
					in:artIn={{ d: direction }}
					out:artOut={{ d: direction }}
				>
					{#if step === "intro"}
						<BotArtIntro />
					{:else if step === "how1"}
						<BotArtServer />
					{:else if step === "how2"}
						<BotArtChat />
					{:else if step === "how3"}
						<BotArtApproval />
					{:else if step === "connect"}
						<!-- Terminal art: install command -->
						<div class="terminal-art">
							<div class="terminal-chrome">
								<span class="tc-dot r"></span><span class="tc-dot y"></span><span class="tc-dot g"></span>
								<span class="tc-title">Terminal</span>
							</div>
							<div class="terminal-body">
								<div class="terminal-line">
									<span class="t-prompt">$ </span><span class="t-cmd"
										>atlas bot install {sshTarget.trim() || "user@host"}</span
									>
								</div>
								{#if connecting}
									<div class="terminal-line t-muted">Connecting…</div>
								{:else if connected}
									<div class="terminal-line t-success">Bot computer ready.</div>
								{:else if connectError}
									<div class="terminal-line t-error">{connectError}</div>
								{:else}
									<div class="terminal-line t-muted terminal-cursor">
										<span class="cursor-block"></span>
									</div>
								{/if}
							</div>
						</div>
					{:else if step === "engine"}
						<!-- Engine logo showcase -->
						<div class="engine-art">
							{#each integrations as agent (agent.id)}
								{@const logo = agentLogo(agent.id)}
								{@const sel = selectedEngine === agent.id}
								<div class="ea-card" class:sel>
									<div class="ea-logo">
										{#if logo}
											{@html logo.dark ?? logo.light}
										{:else}
											<span class="ea-fallback">{agent.label[0]}</span>
										{/if}
									</div>
									{#if sel}
										<div class="ea-check"><Check size={14} /></div>
									{/if}
								</div>
							{/each}
						</div>
					{:else if step === "ready"}
						<!-- BotListRow appearing with spring -->
						<div class="ready-art">
							<BotListRow bot={mockBot} />
						</div>
					{/if}
				</div>
			{/key}
		</div>
	</div>
</div>

<style>
	/* ─── Shell ─────────────────────────────────────────────────── */
	.overlay {
		position: fixed;
		inset: 0;
		z-index: 1600;
		background: var(--ca-surface-base);
		display: flex;
		flex-direction: column;
		min-width: 600px;
	}

	/* ─── Top bar ────────────────────────────────────────────────── */
	.top-bar {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: 0;
		height: 36px;
	}

	.progress-track {
		flex: 1;
		height: 3px;
		background: var(--ca-surface-sunken);
	}

	.progress-fill {
		height: 100%;
		background: var(--ca-accent, #4f8ef7);
		transition: width 400ms ease;
	}

	.skip-btn {
		flex-shrink: 0;
		padding: 8px 20px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 13px;
		cursor: pointer;
		border-radius: 0;
		transition: color 140ms;
	}

	.skip-btn:hover {
		color: var(--ca-text-primary);
	}

	/* ─── Body columns ───────────────────────────────────────────── */
	.body {
		flex: 1;
		display: grid;
		grid-template-columns: 45fr 55fr;
		min-height: 0;
		overflow: hidden;
	}

	/* ─── Left panel ─────────────────────────────────────────────── */
	.left-panel {
		position: relative;
		display: flex;
		align-items: center;
		overflow: hidden;
		padding: 0 48px 0 72px;
	}

	@media (max-width: 1200px) {
		.left-panel {
			padding: 0 32px 0 48px;
		}
	}

	.text-block {
		width: 100%;
		max-width: 480px;
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	/* Stagger animation: each direct child fades + lifts in */
	.text-block > * {
		animation: child-in 320ms calc(var(--stagger, 0) * 60ms) both;
	}

	@keyframes child-in {
		from {
			opacity: 0;
			transform: translateY(6px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.text-block > * {
			animation: none;
		}
	}

	/* Step label */
	.step-label {
		display: block;
		font-size: 13px;
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-accent, #4f8ef7);
		margin-bottom: 14px;
	}

	.step-label.setup {
		color: var(--ca-text-secondary);
	}

	/* Headline */
	.headline {
		font-size: 46px;
		font-weight: 700;
		line-height: 1.08;
		letter-spacing: -0.03em;
		color: var(--ca-text-primary);
		margin: 0 0 20px;
	}

	@media (max-width: 1300px) {
		.headline {
			font-size: 38px;
		}
	}

	/* Body text */
	.body-text {
		font-size: 18px;
		line-height: 1.6;
		color: var(--ca-text-secondary);
		margin: 0 0 24px;
		max-width: 420px;
	}

	@media (max-width: 1300px) {
		.body-text {
			font-size: 16px;
		}
	}

	/* Bullet list */
	.bullets {
		list-style: none;
		padding: 0;
		margin: 0 0 32px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.bullets li {
		font-size: 15px;
		color: var(--ca-text-secondary);
		display: flex;
		align-items: flex-start;
		gap: 10px;
		line-height: 1.4;
	}

	.bullets li::before {
		content: "";
		flex-shrink: 0;
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--ca-accent, #4f8ef7);
		margin-top: 6px;
		opacity: 0.7;
	}

	/* ─── Connect form ────────────────────────────────────────────── */
	.form-block {
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-bottom: 28px;
	}

	.code-block {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--ca-surface-sunken);
		border: 1px solid var(--ca-border-subtle);
		border-radius: 8px;
		padding: 11px 14px;
		font-family: monospace;
		font-size: 13px;
		color: var(--ca-text-primary);
	}

	.code-block code {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.copy-btn {
		flex-shrink: 0;
		padding: 2px 4px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--ca-text-secondary);
		border-radius: 4px;
		display: flex;
		align-items: center;
	}

	.copy-btn:hover {
		color: var(--ca-text-primary);
		background: var(--ca-vibrancy-fill);
	}

	.field-label {
		display: block;
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	.field-wrap {
		border-radius: 8px;
	}

	.field-wrap.shake {
		animation: field-shake 420ms ease;
	}

	@keyframes field-shake {
		0%,
		100% {
			transform: translateX(0);
		}
		20%,
		60% {
			transform: translateX(-6px);
		}
		40%,
		80% {
			transform: translateX(6px);
		}
	}

	.ssh-input {
		width: 100%;
		box-sizing: border-box;
		padding: 11px 14px;
		border-radius: 8px;
		border: 1.5px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		font-size: 15px;
		outline: none;
		transition: border-color 150ms;
	}

	.ssh-input:focus {
		border-color: var(--ca-accent, #4f8ef7);
	}

	.ssh-input:disabled {
		opacity: 0.5;
	}

	.field-error {
		font-size: 13px;
		color: var(--ca-danger, #ef4444);
		margin: 0;
	}

	.connect-success {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 14px;
		color: var(--ca-semantic-success);
	}

	/* ─── Engine list ─────────────────────────────────────────────── */
	.engine-list {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-bottom: 28px;
	}

	.engine-card {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 14px 16px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		border-radius: 10px;
		cursor: pointer;
		text-align: left;
		transition:
			border-color 160ms,
			box-shadow 160ms,
			transform 120ms;
	}

	.engine-card:hover:not(:disabled) {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow: 0 2px 12px rgba(79, 142, 247, 0.12);
	}

	.engine-card:active:not(:disabled) {
		transform: scale(0.99);
	}

	.engine-card.selected {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow:
			0 0 0 3px rgba(79, 142, 247, 0.15),
			0 4px 16px rgba(79, 142, 247, 0.12);
		transform: translateY(-1px);
	}

	.engine-card:disabled {
		cursor: default;
		opacity: 0.6;
	}

	.engine-logo {
		flex-shrink: 0;
		width: 40px;
		height: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.engine-logo :global(svg) {
		width: 40px;
		height: 40px;
	}

	.engine-logo-fallback {
		width: 40px;
		height: 40px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		display: flex;
		align-items: center;
		justify-content: center;
		font-weight: 700;
		font-size: 18px;
		color: var(--ca-text-secondary);
	}

	.engine-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.engine-name {
		font-size: 15px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}

	.engine-auth {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}

	.engine-status {
		flex-shrink: 0;
		font-size: 11.5px;
		font-weight: 600;
		padding: 4px 9px;
		border-radius: 5px;
		border: none;
		cursor: default;
	}

	.engine-status.ready {
		background: rgba(34, 197, 94, 0.12);
		color: var(--ca-semantic-success);
	}

	.engine-status.sign-in {
		display: flex;
		align-items: center;
		gap: 5px;
		background: rgba(245, 158, 11, 0.1);
		color: var(--ca-warning, #f59e0b);
		cursor: pointer;
		font-family: monospace;
		font-size: 10.5px;
		font-weight: 500;
		max-width: 220px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.engine-empty {
		font-size: 14px;
		color: var(--ca-text-secondary);
		text-align: center;
		padding: 20px;
	}

	.engine-empty code {
		background: var(--ca-surface-sunken);
		padding: 1px 5px;
		border-radius: 3px;
		font-size: 12px;
	}

	/* ─── Actions row ────────────────────────────────────────────── */
	.actions {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	.action-group {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.primary-btn {
		height: 48px;
		padding: 0 32px;
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		border: none;
		border-radius: 10px;
		font-size: 15px;
		font-weight: 600;
		cursor: pointer;
		transition:
			opacity 150ms,
			transform 120ms;
	}

	.primary-btn:hover:not(:disabled) {
		opacity: 0.88;
	}

	.primary-btn:active:not(:disabled) {
		transform: scale(0.98);
	}

	.primary-btn:disabled {
		opacity: 0.45;
		cursor: default;
	}

	.back-btn {
		display: flex;
		align-items: center;
		gap: 3px;
		padding: 8px 4px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 14px;
		cursor: pointer;
		border-radius: 6px;
		transition: color 120ms;
	}

	.back-btn:hover {
		color: var(--ca-text-primary);
	}

	.text-link {
		padding: 8px 4px;
		background: none;
		border: none;
		color: var(--ca-text-secondary);
		font-size: 14px;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 3px;
		transition: color 120ms;
	}

	.text-link:hover {
		color: var(--ca-text-primary);
	}

	/* ─── Right panel ────────────────────────────────────────────── */
	.right-panel {
		position: relative;
		overflow: hidden;
		background: var(--ca-surface-sunken);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.art-zone {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		/* Per-step accent glow */
		background: radial-gradient(ellipse 70% 60% at 50% 55%, var(--art-glow, transparent), transparent 70%);
	}

	/* ─── Terminal art (connect step) ────────────────────────────── */
	.terminal-art {
		width: min(520px, 90%);
		border-radius: 10px;
		background: #0d0d0d;
		box-shadow: 0 8px 40px rgba(0, 0, 0, 0.4);
		overflow: hidden;
	}

	.terminal-chrome {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 14px;
		background: #1a1a1a;
		border-bottom: 1px solid #2a2a2a;
	}

	.tc-dot {
		width: 12px;
		height: 12px;
		border-radius: 50%;
	}
	.tc-dot.r {
		background: #ff5f57;
	}
	.tc-dot.y {
		background: #febc2e;
	}
	.tc-dot.g {
		background: #28c840;
	}

	.tc-title {
		flex: 1;
		text-align: center;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}

	.terminal-body {
		padding: 18px 20px 24px;
		font-family: monospace;
		font-size: 13px;
		line-height: 1.7;
		min-height: 80px;
	}

	.terminal-line {
		color: var(--ca-text-primary);
		margin: 0;
	}

	.t-prompt {
		color: var(--ca-semantic-success);
	}

	.t-cmd {
		color: var(--ca-text-primary);
	}

	.t-muted {
		color: var(--ca-text-secondary);
	}

	.t-success {
		color: var(--ca-semantic-success);
	}

	.t-error {
		color: var(--ca-status-error, var(--ca-semantic-error));
	}

	.cursor-block {
		display: inline-block;
		width: 8px;
		height: 14px;
		background: #e2e8f0;
		vertical-align: text-bottom;
		animation: cursor-blink 1s step-end infinite;
	}

	@keyframes cursor-blink {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0;
		}
	}

	/* ─── Engine art ─────────────────────────────────────────────── */
	.engine-art {
		display: grid;
		grid-template-columns: repeat(2, 80px);
		gap: 20px;
	}

	.ea-card {
		width: 80px;
		height: 80px;
		border-radius: 18px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		display: flex;
		align-items: center;
		justify-content: center;
		position: relative;
		transition:
			border-color 200ms,
			box-shadow 200ms,
			transform 200ms;
	}

	.ea-card.sel {
		border-color: var(--ca-accent, #4f8ef7);
		box-shadow: 0 0 0 3px rgba(79, 142, 247, 0.2);
		transform: scale(1.06);
	}

	.ea-card :global(svg) {
		width: 44px;
		height: 44px;
	}

	.ea-fallback {
		font-size: 24px;
		font-weight: 700;
		color: var(--ca-text-secondary);
	}

	.ea-check {
		position: absolute;
		bottom: -6px;
		right: -6px;
		width: 22px;
		height: 22px;
		border-radius: 50%;
		background: var(--ca-accent, #4f8ef7);
		color: var(--ca-surface-base);
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
	}

	/* ─── Ready art ──────────────────────────────────────────────── */
	.ready-art {
		width: min(380px, 88%);
		animation: ready-spring 480ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
	}

	@keyframes ready-spring {
		from {
			opacity: 0;
			transform: translateY(20px) scale(0.94);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.ready-art {
			animation: none;
		}
		.cursor-block {
			animation: none;
		}
	}
</style>
