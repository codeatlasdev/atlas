<script lang="ts">
	import { cubicOut, cubicIn } from "svelte/easing";
	import type { TransitionConfig } from "svelte/transition";
	import BotArtIntro from "#lib/bots/BotArtIntro.svelte";
	import { SEEN_KEY } from "#lib/bots/crewOnboarding.svelte.ts";
	import BotArtHow from "#lib/bots/BotArtHow.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import BotListRow from "#lib/bots/BotListRow.svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
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
	/** Which line of the how-it-works story the art is on (set by BotArtHow). */
	let howBeat = $state(0);
	const STORY = [
		{ title: "You give the order", text: "Type what you want, like a message." },
		{ title: "It works on your server", text: "All night, even with your Mac asleep." },
		{ title: "It asks before anything risky", text: "Allow or deny from your Mac, phone or Telegram." },
		{ title: "You wake up to results", text: "Every step is logged, ready to read." },
	];
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
		name: "My First Crewmate",
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
			label: "1 of 2",
			headline: "Meet your crew",
			body: "Your crew sails while you sleep.",
			bullets: [
				"Your ship, your rules — runs on your server, no account",
				"Hand it the wheel — it keeps working while your Mac sleeps",
				"It flags you before anything risky",
			],
			glow: "rgba(61, 91, 255, 0.10)",
		},
		how: {
			label: "2 of 2",
			headline: "One night with your crew",
			body: "This is what a crewmate does while you're away.",
			bullets: undefined,
			glow: "rgba(61, 91, 255, 0.10)",
		},
		connect: {
			headline: "Launch your ship",
			body: "Run this command on your Linux server, then enter its address below.",
			glow: "rgba(61, 91, 255, 0.08)",
		},
		engine: {
			headline: "Choose an engine",
			body: "Select the AI your bot will use. Each engine runs on your own subscription or API key.",
			glow: "rgba(61, 91, 255, 0.10)",
		},
		ready: {
			headline: "Your first crewmate is ready",
			body: "Open a chat to send your first task. You can add more crewmates and customise them from the workspace.",
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
			localStorage.setItem(SEEN_KEY, "1");
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
					name: "My First Crewmate",
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
	aria-label="Atlas Crew setup"
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
					<!-- Info step label "N of 2" -->
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

					<!-- The how-it-works story, lit in sync with the art -->
					{#if step === "how"}
						<ol class="story" style:--stagger={3}>
							{#each STORY as line, i (line.title)}
								<li class:on={howBeat === i} class:past={howBeat > i}>
									<b>{line.title}</b>
									<span>{line.text}</span>
								</li>
							{/each}
						</ol>
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
									<ShipFlag size={18} />
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
					{:else if step === "how"}
						<BotArtHow bind:beat={howBeat} />
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
						<!-- Hero bot card: large face + name -->
						{@const readyLogo = agentLogo(selectedEngine || mockBot.agent)}
						<div class="ready-hero">
							<div class="hero-face-wrap">
								<div class="hero-bob">
									<BotFace seed={mockBot.id} look={previewLook} state="idle" size={140} />
								</div>
								{#if readyLogo}
									<span class="hero-badge">{@html readyLogo.dark ?? readyLogo.light}</span>
								{/if}
							</div>
							<div class="hero-info">
								<span class="hero-name">{mockBot.name}</span>
								<span class="hero-sub">No tasks yet</span>
							</div>
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
		/* macOS: the traffic lights sit over the top ~40px of the window; keep the bar clear of them. */
		margin-top: 40px;
	}
	:global(html.opaque-window) .top-bar {
		margin-top: 0;
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
		display: grid;
		align-content: center;
		overflow: hidden;
		/* Everything inside scales with the column (cqi), so a full-screen window gets a full-size page,
		   not the laptop layout floating in a corner. */
		container-type: inline-size;
		padding: 64px clamp(32px, 6cqi, 96px) 64px clamp(48px, 9cqi, 140px);
	}

	/* entering + exiting text-blocks both live in cell 1/1 — no width competition */
	.left-panel > * {
		grid-area: 1 / 1;
		min-width: 0;
	}

	.text-block {
		width: 100%;
		max-width: clamp(420px, 62cqi, 760px);
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
		font-size: clamp(12px, 1.25cqi, 16px);
		font-weight: 600;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-accent, #4f8ef7);
		margin-bottom: clamp(12px, 1.6cqi, 22px);
	}

	.step-label.setup {
		color: var(--ca-text-secondary);
	}

	/* Headline */
	.headline {
		font-size: clamp(34px, 5.4cqi, 76px);
		font-weight: 700;
		line-height: 1.05;
		letter-spacing: -0.03em;
		color: var(--ca-text-primary);
		margin: 0 0 clamp(16px, 2.2cqi, 32px);
	}

	/* Body text */
	.body-text {
		font-size: clamp(16px, 1.9cqi, 26px);
		line-height: 1.5;
		color: var(--ca-text-secondary);
		margin: 0 0 clamp(20px, 3cqi, 44px);
		max-width: 34em;
	}

	.story {
		list-style: none;
		margin: 20px 0 36px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.story li {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 10px 14px;
		border-left: 2px solid var(--ca-border);
		opacity: 0.45;
		transition:
			opacity 400ms ease,
			border-color 400ms ease,
			background 400ms ease;
	}
	.story li.past {
		opacity: 0.75;
	}
	.story li.on {
		opacity: 1;
		border-left-color: #3d5bff;
		background: rgb(61 91 255 / 0.08);
	}
	.story b {
		font-size: 15px;
	}
	.story span {
		font-size: 13px;
		color: var(--ca-text-secondary);
	}

	/* Bullet list */
	.bullets {
		list-style: none;
		padding: 0;
		margin: 0 0 clamp(28px, 4cqi, 56px);
		display: flex;
		flex-direction: column;
		gap: clamp(10px, 1.3cqi, 18px);
	}

	.bullets li {
		font-size: clamp(14px, 1.55cqi, 21px);
		color: var(--ca-text-secondary);
		display: flex;
		align-items: flex-start;
		gap: 10px;
		line-height: 1.4;
	}

	.bullets li::before {
		content: "";
		flex-shrink: 0;
		width: 0.35em;
		height: 0.35em;
		border-radius: 50%;
		background: var(--ca-accent, #4f8ef7);
		margin-top: 0.5em;
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
		border-radius: 999px;
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
		display: grid;
		place-items: center;
	}

	/* entering + exiting art-zones both live in cell 1/1 */
	.right-panel > * {
		grid-area: 1 / 1;
		width: 100%;
		height: 100%;
	}

	.art-zone {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		/* Per-step accent glow */
		background: radial-gradient(ellipse 80% 70% at 50% 55%, var(--art-glow, transparent), transparent 75%);
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

	/* ─── Ready hero card ────────────────────────────────────────── */
	.ready-hero {
		width: min(360px, 88%);
		padding: 36px 32px 28px;
		background: var(--ca-surface-base);
		border: 1.5px solid var(--ca-border-subtle);
		border-radius: 20px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 20px;
		box-shadow: 0 12px 48px rgba(0, 0, 0, 0.28);
		animation: hero-spring 560ms cubic-bezier(0.34, 1.56, 0.64, 1) both;
	}

	@keyframes hero-spring {
		from {
			opacity: 0;
			transform: translateY(32px) scale(0.88);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.hero-face-wrap {
		position: relative;
		flex-shrink: 0;
	}

	/* periodic hop after the spring entrance */
	.hero-bob {
		animation: face-hop 3.6s ease-in-out infinite;
		animation-delay: 700ms;
	}

	@keyframes face-hop {
		0%,
		75%,
		100% {
			transform: translateY(0);
		}
		88% {
			transform: translateY(-9px);
		}
	}

	.hero-badge {
		position: absolute;
		bottom: -4px;
		right: -4px;
		width: 30px;
		height: 30px;
		background: var(--ca-surface-base);
		border: 2.5px solid var(--ca-surface-base);
		border-radius: 9px;
		display: flex;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		box-shadow: 0 2px 6px rgba(0, 0, 0, 0.18);
	}

	.hero-badge :global(svg) {
		width: 22px;
		height: 22px;
	}

	.hero-info {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
		text-align: center;
	}

	.hero-name {
		font-size: 26px;
		font-weight: 700;
		color: var(--ca-text-primary);
		letter-spacing: -0.02em;
	}

	.hero-sub {
		font-size: 14px;
		color: var(--ca-text-secondary);
	}

	@media (prefers-reduced-motion: reduce) {
		.ready-hero {
			animation: none;
		}
		.hero-bob {
			animation: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.cursor-block {
			animation: none;
		}
	}
</style>
