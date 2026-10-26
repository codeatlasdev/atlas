<script lang="ts">
	import { cubicOut, cubicIn } from "svelte/easing";
	import type { TransitionConfig } from "svelte/transition";
	import BotArtIntro from "#lib/bots/BotArtIntro.svelte";
	import { SEEN_KEY } from "#lib/bots/crewOnboarding.svelte.ts";
	import BotArtHow from "#lib/bots/BotArtHow.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import ShipFlag from "#lib/bots/ShipFlag.svelte";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { randomLook } from "#lib/bots/look.ts";
	import { reducedMotion } from "#lib/bots/motion.ts";
	import { agentLogo } from "#lib/agentLogos.ts";
	import {
		INITIAL_STEP,
		nextStep,
		prevStep,
		infoStepIndex,
		stepProgress,
		type OnboardingStep,
	} from "#lib/bots/botsOnboarding.ts";
	import { Copy, Check, ChevronLeft } from "@lucide/svelte";
	import { toast } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { b64decode } from "#lib/b64.ts";
	import { parseInstallLine, INSTALL_STEPS, type InstallPhase } from "#lib/bots/installSteps.ts";
	import { botTarget, parseSSHTarget } from "#lib/bots/sshTarget.ts";
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
	let sshPort = $state(22);
	/** Phases that have completed (ticked in the checklist). */
	let installedPhases = $state(new Set<InstallPhase>());
	/** The phase currently running (spinner). */
	let activePhase = $state<InstallPhase | null>(null);
	/** Phase that failed (shown in red). */
	let failedPhase = $state<InstallPhase | null>(null);
	/** Last error line from stderr. */
	let lastErrLine = $state("");
	let installRunning = $state(false);
	let installDone = $state(false);
	/** Host key of the ship launched in this onboarding; the first crewmate is created there. */
	let launchedKey = $state("");
	let fieldShake = $state(false);

	// ─── connect step: SSH key ────────────────────────────────────────────────
	let keyChecked = $state(false);
	let hasKey = $state(false);
	let creatingKey = $state(false);
	let keyError = $state("");

	// ─── connect step: auth flows ─────────────────────────────────────────────
	/** "authorize" = Permission denied → copy key via password.
	 *  "sudo" = sudo needs password. "none" = not in an auth flow. */
	let authMode = $state<"none" | "authorize" | "sudo">("none");
	let authPassword = $state("");
	let authRunning = $state(false);
	let authError = $state("");

	// ─── engine step ──────────────────────────────────────────────────────────
	type EnginePhase =
		| 'idle'
		| 'checking'
		| 'not_installed'
		| 'installing'
		| 'install_error'
		| 'needs_signin'
		| 'ready';

	interface EngineStatus { phase: EnginePhase; error?: string; log?: string }

	let engineStatus = $state<Record<string, EngineStatus>>({});
	let installLog = $state(""); // accumulated install stdout/stderr
	let signinActive = $state<string | null>(null); // agent being signed in
	let signinPtyId = $state<string | null>(null);
	let signinOutput = $state(""); // accumulated PTY output (ANSI stripped)
	let signinInput = $state(""); // current text field value
	let apiKeyValue = $state("");
	let apiKeyApplying = $state(false);
	let apiKeyError = $state("");
	let apiKeyMode = $state(false); // true = show key field
	let selectedEngine = $state("");

	// ─── ready step ───────────────────────────────────────────────────────────
	let creatingBot = $state(false);
	const previewLook = randomLook();

	// ─── derived ──────────────────────────────────────────────────────────────
	const integrations = $derived(
		agents.list.filter((a) => ["claude", "opencode", "codex", "omp"].includes(a.id)),
	);

	const engineReady = $derived(
		selectedEngine !== "" && engineStatus[selectedEngine]?.phase === "ready"
	);

	const shipSshTarget = $derived(
		launchedKey ? launchedKey.replace(/^ssh:/, "") : ""
	);

	const installingAny = $derived(Object.values(engineStatus).some(s => s.phase === 'installing'));


	const targetValid = $derived(
		sshTarget.trim() === "" ? false : parseSSHTarget(`${sshTarget.trim()}:${sshPort}`).ok
	);

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

	// ─── engine auth constants ────────────────────────────────────────────────
	const AGENT_AUTH_TYPE: Record<string, 'apikey' | 'both'> = {
		omp:      'apikey',
		claude:   'both',
		codex:    'both',
		opencode: 'both',
	};
	const AGENT_KEY_LABEL: Record<string, string> = {
		omp:      'OpenRouter or OpenAI key',
		claude:   'Anthropic API key (optional)',
		codex:    'OpenAI API key',
		opencode: 'Provider API key',
	};
	const AGENT_KEY_ENV: Record<string, string> = {
		omp:      'OPENROUTER_API_KEY',
		claude:   'ANTHROPIC_API_KEY',
		codex:    'OPENAI_API_KEY',
		opencode: 'OPENAI_API_KEY',
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
			headline: "Meet your Mugiwaras",
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
		body: "You need a Linux server you can SSH into.",
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

	// Detect SSH key when the connect step becomes active
	$effect(() => {
		if (step === "connect" && !keyChecked) {
			invoke<string | null>("ssh_key_find").then((p) => {
				hasKey = p !== null;
				keyChecked = true;
			});
		}
	});

	// ─── connect step actions ─────────────────────────────────────────────────

	async function createSshKey() {
		creatingKey = true;
		keyError = "";
		try {
			await invoke("ssh_keygen");
			hasKey = true;
		} catch (e) {
			keyError = errText(e);
		} finally {
			creatingKey = false;
		}
	}

	/** Core install runner. Accepts an optional sudo password (piped securely). */
	async function launchShip(sudoPassword?: string) {
		const target = sshTarget.trim();
		if (!target) {
			fieldShake = true;
			setTimeout(() => (fieldShake = false), 500);
			return;
		}
		installRunning = true;
		installDone = false;
		failedPhase = null;
		lastErrLine = "";
		installedPhases = new Set<InstallPhase>();
		activePhase = null;
		authMode = "none";
		authError = "";

		const unlisten = await listen<{ line: string; is_err: boolean }>(
			"bot://install-line",
			(e) => {
				const phase = parseInstallLine(e.payload.line);
				if (!phase) return;
				if (phase === "done") {
					installedPhases = new Set(INSTALL_STEPS.map((s) => s.phase));
					activePhase = null;
				} else {
					if (activePhase) installedPhases = new Set([...installedPhases, activePhase]);
					activePhase = phase;
				}
			},
		);

		try {
			const args: Record<string, unknown> = { target, port: sshPort !== 22 ? sshPort : null };
			if (sudoPassword !== undefined) args.sudoPassword = sudoPassword;
			await invoke("bot_install", args);
			const ship = botTarget(target);
			await bots.addHost(ship, ship);
			launchedKey = `ssh:${ship}`;
			installedPhases = new Set(INSTALL_STEPS.map((s) => s.phase));
			activePhase = null;
			installDone = true;
			setTimeout(advance, 900);
		} catch (e) {
			const err = errText(e);
			failedPhase = activePhase;
			lastErrLine = err;
			if (err.includes("Permission denied (publickey)")) {
				authMode = "authorize";
			} else if (err.includes("asks for a password") || err.includes("passwordless sudo") || err.includes("sudo password rejected")) {
				authMode = "sudo";
			}
			fieldShake = true;
			setTimeout(() => (fieldShake = false), 500);
		} finally {
			unlisten();
			installRunning = false;
		}
	}

	/** Authorize this Mac's key on the server via password, then retry install. */
	async function authorizeAndRetry() {
		authRunning = true;
		authError = "";
		const pw = authPassword;
		authPassword = ""; // clear from state immediately
		try {
			await invoke("ssh_authorize_key", { target: sshTarget.trim(), port: sshPort !== 22 ? sshPort : null, password: pw });
			authMode = "none";
			await launchShip();
		} catch (e) {
			authError = errText(e);
			authMode = "authorize"; // stay in auth mode so user can retry
		} finally {
			authRunning = false;
		}
	}

	async function copyInstallCmd() {
		await navigator.clipboard
			.writeText(`atlas bot install ${sshTarget.trim() || "user@host"}`)
			.catch(() => {});
		toast.success("Copied");
	}

	// ─── engine step: check agents on ship ───────────────────────────────────
	$effect(() => {
		if (step !== 'engine' || !launchedKey) return;
		for (const agent of integrations) {
			const cur = engineStatus[agent.id];
			if (!cur || cur.phase === 'idle') {
				engineStatus[agent.id] = { phase: 'checking' };
				invoke<{ installed: boolean; signed_in: boolean }>('bot_agent_check', {
					hostKey: launchedKey,
					agentId: agent.id,
				}).then((r) => {
					engineStatus[agent.id] = {
						phase: r.installed ? (r.signed_in ? 'ready' : 'needs_signin') : 'not_installed',
					};
				}).catch((e) => {
					engineStatus[agent.id] = { phase: 'not_installed', error: errText(e) };
				});
			}
		}
	});

	async function installEngine(agentId: string) {
		engineStatus[agentId] = { phase: 'installing' };
		installLog = '';
		const unlisten = await listen<{ line: string; is_err: boolean }>('bot://install-line', (e) => {
			installLog += (installLog ? '\n' : '') + e.payload.line;
		});
		try {
			await invoke('bot_agent_install', { hostKey: launchedKey, agentId });
			const r = await invoke<{ installed: boolean; signed_in: boolean }>('bot_agent_check', {
				hostKey: launchedKey,
				agentId,
			});
			engineStatus[agentId] = {
				phase: r.installed ? (r.signed_in ? 'ready' : 'needs_signin') : 'install_error',
				error: r.installed ? undefined : 'Install finished but binary not found',
			};
		} catch (e) {
			engineStatus[agentId] = { phase: 'install_error', error: errText(e) };
		} finally {
			unlisten();
		}
	}

	async function openSignInTerminal(agentId: string) {
		if (!shipSshTarget || signinPtyId) return;
		signinActive = agentId;
		apiKeyMode = false;
		signinOutput = '';
		signinInput = '';
		signinPtyId = null;
		const authCmd: Record<string, string> = {
			claude: 'claude',
			codex: 'codex',
			opencode: 'opencode',
			omp: 'omp',
		};
		const cmd = authCmd[agentId] ?? agentId;
		try {
			const r = await invoke<{ pty: string }>('host_call', {
				cmd: 'pty_create',
				args: { argv: ['ssh', '-t', shipSshTarget, cmd], cwd: '/' },
			});
			signinPtyId = r.pty;
			await invoke('host_call', { cmd: 'pty_attach', args: { pty: r.pty } });
		} catch (e) {
			signinOutput = errText(e);
		}
	}

	function stripAnsi(s: string): string {
		return s.replace(/\x1b(?:\[[\d;]*[A-Za-z]|\][^\x07]*(?:\x07|\x1b\\))/g, '')
		        .replace(/[^\n]*\r([^\n])/g, '$1');
	}

	$effect(() => {
		if (!signinPtyId) return;
		const id = signinPtyId;
		let off: (() => void) | null = null;
		listen<{ data: string; replay?: boolean }>(`pty://${id}`, (e) => {
			const text = b64decode(e.payload.data);
			signinOutput += stripAnsi(text);
		}).then(fn => { off = fn; });
		const exitOff = listen<{ ev: string; pty?: string }>('host://event', (e) => {
			if (e.payload.ev === 'pty_exit' && e.payload.pty === id) {
				void recheckSignin();
			}
		});
		return () => {
			off?.();
			exitOff.then(fn => fn());
		};
	});

	async function recheckSignin() {
		if (!signinActive || !launchedKey) return;
		const r = await invoke<{ installed: boolean; signed_in: boolean }>('bot_agent_check', {
			hostKey: launchedKey,
			agentId: signinActive,
		}).catch(() => ({ installed: true, signed_in: false }));
		if (r.signed_in) {
			engineStatus[signinActive] = { phase: 'ready' };
			signinPtyId = null;
			signinActive = null;
		}
	}

	async function sendSigninKey(e: KeyboardEvent) {
		if (!signinPtyId) return;
		if (e.key === 'Enter') {
			await invoke('host_call', { cmd: 'pty_write', args: { pty: signinPtyId, data: btoa(signinInput + '\r') } });
			signinInput = '';
		}
	}

	async function applyApiKey() {
		if (!apiKeyValue.trim() || !signinActive || !launchedKey) return;
		apiKeyApplying = true;
		apiKeyError = '';
		const agentId = signinActive;
		const envVar = AGENT_KEY_ENV[agentId] ?? 'API_KEY';
		const content = `${envVar}=${apiKeyValue.trim()}\n`;
		const dir = '/var/lib/atlas-bot/.atlas/agent-env';
		try {
			await invoke('host_call', { cmd: 'fs_mkdir', args: { host: launchedKey, path: dir } });
			const { b64encode } = await import('#lib/b64.ts');
			await invoke('host_call', { cmd: 'fs_write', args: { host: launchedKey, path: `${dir}/${agentId}.env`, data: b64encode(content) } });
			if (agentId === 'codex') {
				await invoke('host_call', { cmd: 'exec', args: {
					host: launchedKey, cwd: '/',
					argv: ['sh', '-c', `export PATH="$HOME/.npm-global/bin:$PATH"; echo "${apiKeyValue.trim()}" | codex login --with-api-key`],
					timeout_ms: 30000,
				}});
			}
			apiKeyValue = '';
			const r = await invoke<{ installed: boolean; signed_in: boolean }>('bot_agent_check', {
				hostKey: launchedKey,
				agentId,
			});
			if (r.signed_in) {
				engineStatus[agentId] = { phase: 'ready' };
				signinActive = null;
				apiKeyMode = false;
			} else {
				apiKeyError = 'Key set but agent does not recognise it yet. Try the interactive flow.';
			}
		} catch (e) {
			apiKeyError = errText(e);
		} finally {
			apiKeyApplying = false;
		}
	}

	function handleEngineCardClick(agentId: string) {
		const status = engineStatus[agentId];
		if (!status) return;
		switch (status.phase) {
			case 'not_installed':
			case 'install_error':
				void installEngine(agentId);
				break;
			case 'needs_signin':
				signinActive = agentId;
				signinPtyId = null;
				signinOutput = '';
				signinInput = '';
				apiKeyValue = '';
				apiKeyError = '';
				if (AGENT_AUTH_TYPE[agentId] === 'apikey') {
					apiKeyMode = true;
				} else {
					apiKeyMode = false;
					void openSignInTerminal(agentId);
				}
				break;
			case 'ready':
				selectEngine(agentId);
				break;
			case 'checking':
			case 'installing':
				break;
		}
	}

	function engineStatusLabel(phase: EnginePhase): string {
		switch (phase) {
			case 'idle': return 'Checking…';
			case 'checking': return 'Checking…';
			case 'not_installed': return 'Install';
			case 'installing': return 'Installing…';
			case 'install_error': return 'Retry';
			case 'needs_signin': return 'Sign in';
			case 'ready': return 'Ready';
		}
	}

	// ─── engine / ready step actions ─────────────────────────────────────────

	function selectEngine(id: string) {
		selectedEngine = id;
		// Automatically advance to ready after a short hold so selection is visible
		setTimeout(advance, 560);
	}

	async function openChat() {
		if (creatingBot) return;
		// The ship launched here, else the first remote ship; hosts[0] is this Mac, never the crew's home.
		const hostKey = launchedKey || bots.hosts.find((h) => h.key !== "")?.key || "";
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
						{#if !keyChecked}
							<div class="key-checking">
								<span class="spin-dot" aria-hidden="true"></span>
								<span>Checking SSH key…</span>
							</div>
						{:else if !hasKey}
							<!-- No SSH key found -->
							<p class="key-missing-text">
								Atlas signs in to your server with an SSH key, like a password
								that never leaves your Mac. You don't have one yet.
							</p>
							{#if keyError}
								<p class="field-error" role="alert">{keyError}</p>
							{/if}
						{:else}
							<!-- Has SSH key: show address field -->
							<label class="field-label" for="ssh-target">SSH address</label>
							<div class="field-wrap" class:shake={fieldShake}>
								<input
									id="ssh-target"
									class="ssh-input"
									bind:value={sshTarget}
									placeholder="ubuntu@my-server.com"
									aria-label="SSH address"
									disabled={installRunning || installDone || authRunning}
									onkeydown={(e) => e.key === "Enter" && launchShip()}
								/>
							</div>
							<p class="field-hint">Uses your Mac's SSH key, the same way <code>ssh {sshTarget.trim() || "user@host"}</code> does.</p>

							<div class="port-row">
								<label class="field-label" for="ssh-port">Port</label>
								<input
									id="ssh-port"
									class="port-input"
									type="number"
									min={1}
									max={65535}
									bind:value={sshPort}
									disabled={installRunning || installDone || authRunning}
								/>
							</div>

							{#if authMode === "authorize"}
								<!-- Permission denied: offer to copy key via password -->
								<div class="auth-block">
									<p class="auth-desc">Your Mac's SSH key isn't authorized on this server yet. Enter the server user's password to add it.</p>
									<input
										type="password"
										class="ssh-input"
										bind:value={authPassword}
										placeholder="Server password"
										aria-label="Server password for SSH key authorization"
										disabled={authRunning}
										onkeydown={(e) => e.key === "Enter" && authorizeAndRetry()}
									/>
									{#if authError}
										<p class="field-error" role="alert">{authError}</p>
									{/if}
								</div>
							{:else if authMode === "sudo"}
								<!-- sudo needs password -->
								<div class="auth-block">
									<p class="auth-desc">This user's sudo needs a password. Enter it to continue the install.</p>
									<input
										type="password"
										class="ssh-input"
										bind:value={authPassword}
										placeholder="Sudo password"
										aria-label="Sudo password"
										disabled={authRunning}
										onkeydown={(e) => e.key === "Enter" && launchShip(authPassword)}
									/>
									{#if authError}
										<p class="field-error" role="alert">{authError}</p>
									{/if}
								</div>
							{:else if lastErrLine}
								<!-- Any failure, including one before the first step marker (e.g. the CLI can't run): never shake silently. -->
								<p class="field-error" role="alert">{lastErrLine}</p>
							{/if}

							{#if installDone}
								<div class="connect-success">
									<ShipFlag size={18} />
									<Check size={15} />
									<span>Installed — connecting…</span>
								</div>
							{/if}

							<!-- Disclosure: prefer terminal -->
							<details class="terminal-disclosure">
								<summary>Prefer the terminal?</summary>
								<div class="code-block">
									<code>atlas bot install {sshTarget.trim() || "user@host"}</code>
									<button class="copy-btn" onclick={copyInstallCmd} aria-label="Copy install command">
										<Copy size={13} />
									</button>
								</div>
								<p class="disclosure-hint">Run on your Mac. It SSHes into the server and sets everything up.</p>
							</details>
						{/if}
					</div>
				{/if}

					<!-- ── Engine step form ───────────────────────────────────── -->
					{#if step === 'engine'}
						<div class="engine-list" style:--stagger={3}>
							{#each integrations as agent (agent.id)}
								{@const status = engineStatus[agent.id] ?? { phase: 'idle' }}
								{@const logo = agentLogo(agent.id)}
								{@const isSel = selectedEngine === agent.id}
								{@const isActive = signinActive === agent.id}
								<button
									class="engine-card"
									class:selected={isSel || isActive}
									onclick={() => handleEngineCardClick(agent.id)}
									aria-pressed={isSel}
									aria-label="{agent.label} – {engineStatusLabel(status.phase)}"
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
										{#if status.phase === 'install_error'}
											<span class="engine-auth err-text">{status.error?.slice(0,60)}</span>
										{:else}
											<span class="engine-auth">{AGENT_KEY_LABEL[agent.id] ?? ''}</span>
										{/if}
									</div>
									<span
										class="engine-status"
										class:badge-ready={status.phase === 'ready'}
										class:badge-action={status.phase === 'not_installed' || status.phase === 'needs_signin'}
										class:badge-busy={status.phase === 'installing' || status.phase === 'checking' || status.phase === 'idle'}
										class:badge-error={status.phase === 'install_error'}
										aria-hidden="true"
									>
										{#if status.phase === 'installing' || status.phase === 'checking' || status.phase === 'idle'}
											<span class="spin-dot"></span>
										{/if}
										{engineStatusLabel(status.phase)}
									</span>
								</button>
							{/each}
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
							{#if !keyChecked}
								<!-- waiting for key check -->
							{:else if !hasKey}
								<button class="primary-btn" onclick={createSshKey} disabled={creatingKey}>
									{creatingKey ? "Creating…" : "Create SSH key"}
								</button>
							{:else if authMode === "authorize"}
								<button
									class="primary-btn"
									onclick={authorizeAndRetry}
									disabled={authRunning || !authPassword.trim()}
								>
									{authRunning ? "Authorizing…" : "Authorize this Mac"}
								</button>
								<button class="text-link" onclick={() => { authMode = "none"; authPassword = ""; }}>Cancel</button>
							{:else if authMode === "sudo"}
								<button
									class="primary-btn"
									onclick={() => launchShip(authPassword)}
									disabled={installRunning || !authPassword.trim()}
								>
									{installRunning ? "Installing…" : "Retry with password"}
								</button>
								<button class="text-link" onclick={() => { authMode = "none"; authPassword = ""; }}>Cancel</button>
							{:else}
								<button
									class="primary-btn"
									onclick={() => launchShip()}
								disabled={installRunning || installDone || !targetValid}
								>
									{installRunning ? "Installing…" : "Launch ship"}
								</button>
								{#if failedPhase && !installRunning && !installDone}
									<button class="text-link" onclick={() => launchShip()}>Retry</button>
								{/if}
								{#if !installRunning && !installDone}
									<button class="text-link" onclick={advance}>Skip for now</button>
								{/if}
							{/if}
						</div>
					{:else if step === 'engine'}
						<div class="action-group">
							{#if signinActive}
								<button class="primary-btn" onclick={() => { signinActive = null; signinPtyId = null; apiKeyMode = false; }}>Back</button>
							{/if}
							<button class="text-link" onclick={finish}>Skip for now</button>
						</div>
						{:else if step === "ready"}
							<button class="primary-btn" onclick={openChat} disabled={creatingBot || !engineReady}>
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
						<!-- Progress checklist -->
						<div class="install-checklist">
							{#each INSTALL_STEPS as s (s.phase)}
								{@const done = installedPhases.has(s.phase)}
								{@const active = activePhase === s.phase}
								{@const failed = failedPhase === s.phase}
								<div
									class="checklist-row"
									class:done
									class:active
									class:failed
								>
									<span class="checklist-icon" aria-hidden="true">
										{#if done}
											<Check size={14} />
										{:else if failed}
											<span class="fail-dot"></span>
										{:else if active}
											<span class="spin-dot"></span>
										{:else}
											<span class="idle-dot"></span>
										{/if}
									</span>
									<span class="checklist-label">{s.label}</span>
								</div>
							{/each}
						</div>
					{:else if step === 'engine'}
						{#if signinActive && (signinPtyId || apiKeyMode)}
							<div class="signin-panel">
								{#if apiKeyMode}
									<div class="signin-key-form">
										<p class="signin-title">Paste your {AGENT_KEY_LABEL[signinActive] ?? 'API key'}</p>
										<p class="signin-hint">It's stored on your ship only, never sent anywhere else.</p>
										<input
											class="ssh-input"
											type="password"
											placeholder="sk-…"
											bind:value={apiKeyValue}
											onkeydown={(e) => e.key === 'Enter' && applyApiKey()}
											aria-label="API key"
										/>
										{#if apiKeyError}<p class="field-error">{apiKeyError}</p>{/if}
										<button class="primary-btn" onclick={applyApiKey} disabled={apiKeyApplying || !apiKeyValue.trim()}>
											{apiKeyApplying ? 'Applying…' : 'Apply'}
										</button>
									</div>
								{:else}
									<div class="signin-terminal">
										<div class="signin-term-output" aria-live="polite" aria-label="Terminal output">
											<pre>{signinOutput || 'Connecting…'}</pre>
										</div>
										<div class="signin-term-input-row">
											<input
												class="signin-term-input"
												type="text"
												placeholder="Type here and press Enter…"
												bind:value={signinInput}
												onkeydown={sendSigninKey}
												aria-label="Terminal input"
											/>
										</div>
									</div>
								{/if}
								{#if AGENT_AUTH_TYPE[signinActive] === 'both'}
									<button class="text-link" onclick={() => { apiKeyMode = !apiKeyMode; }}
										style="font-size:12px;margin-top:8px">
										{apiKeyMode ? 'Use interactive sign-in instead' : 'Paste API key instead'}
									</button>
								{/if}
								<button class="text-link" onclick={recheckSignin} style="font-size:12px">
									Check again
								</button>
							</div>
						{:else if installingAny}
							<div class="engine-install-log">
								<pre>{installLog || 'Starting…'}</pre>
							</div>
						{:else}
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
						{/if}
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

	.field-hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	.port-row {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.port-input {
		width: 90px;
		padding: 8px 10px;
		border-radius: 8px;
		border: 1.5px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		font-size: 14px;
		outline: none;
		transition: border-color 150ms;
	}

	.port-input:focus {
		border-color: var(--ca-accent, #4f8ef7);
	}

	.port-input:disabled {
		opacity: 0.5;
	}


	.field-hint code {
		font-family: monospace;
		font-size: 11.5px;
		background: var(--ca-surface-sunken);
		padding: 1px 4px;
		border-radius: 3px;
	}

	.connect-success {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 14px;
		color: var(--ca-semantic-success);
	}

	/* ─── SSH key missing ────────────────────────────────────────── */
	.key-checking {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 14px;
		color: var(--ca-text-secondary);
		padding: 8px 0;
	}

	.key-missing-text {
		font-size: 15px;
		line-height: 1.55;
		color: var(--ca-text-secondary);
		margin: 0 0 20px;
	}

	/* ─── Auth block (password fields for key-copy / sudo) ─────── */
	.auth-block {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.auth-desc {
		font-size: 13px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	/* ─── Install progress checklist ─────────────────────────────── */
	.install-checklist {
		display: flex;
		flex-direction: column;
		gap: 14px;
		width: min(340px, 88%);
	}

	.checklist-row {
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 15px;
		color: var(--ca-text-secondary);
		transition: color 250ms;
	}

	.checklist-row.done {
		color: var(--ca-text-primary);
	}

	.checklist-row.active {
		color: var(--ca-text-primary);
	}

	.checklist-row.failed {
		color: var(--ca-danger, #ef4444);
	}

	.checklist-icon {
		flex-shrink: 0;
		width: 22px;
		height: 22px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.checklist-row.done .checklist-icon {
		color: var(--ca-semantic-success);
	}

	.idle-dot,
	.spin-dot,
	.fail-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}

	.idle-dot {
		background: var(--ca-outline-subtle);
	}

	.spin-dot {
		border: 2px solid var(--ca-accent, #4f8ef7);
		border-top-color: transparent;
		animation: checklist-spin 700ms linear infinite;
	}

	.fail-dot {
		background: var(--ca-danger, #ef4444);
	}

	@keyframes checklist-spin {
		to { transform: rotate(360deg); }
	}

	/* ─── Disclosure ─────────────────────────────────────────────── */
	.terminal-disclosure {
		margin-top: 4px;
	}

	.terminal-disclosure summary {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		cursor: pointer;
		user-select: none;
		list-style: none;
		padding: 4px 0;
	}

	.terminal-disclosure summary::-webkit-details-marker {
		display: none;
	}

	.terminal-disclosure[open] summary {
		color: var(--ca-text-primary);
		margin-bottom: 8px;
	}

	.disclosure-hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 6px 0 0;
		line-height: 1.5;
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

	/* ─── Engine status badges ──────────────────────────────────── */
	.engine-status.badge-ready { background: rgba(34,197,94,0.12); color: var(--ca-semantic-success); }
	.engine-status.badge-action { background: rgba(79,142,247,0.12); color: var(--ca-accent, #4f8ef7); cursor: pointer; }
	.engine-status.badge-busy { background: var(--ca-surface-sunken); color: var(--ca-text-secondary); display: inline-flex; align-items: center; gap: 5px; }
	.engine-status.badge-error { background: rgba(239,68,68,0.12); color: var(--ca-danger, #ef4444); cursor: pointer; }
	.engine-card.selected .engine-status.badge-action { background: rgba(79,142,247,0.2); }

	.err-text { color: var(--ca-danger, #ef4444) !important; }


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


	/* ─── Engine install log ──────────────────────────────────────────── */
	.engine-install-log {
		width: 90%; height: 100%;
		display: flex; align-items: center; justify-content: center;
		overflow: hidden;
	}
	.engine-install-log pre {
		width: 100%; max-height: 80%;
		overflow-y: auto;
		font-family: monospace; font-size: 11.5px; line-height: 1.6;
		color: var(--ca-text-secondary); white-space: pre-wrap; word-break: break-all;
		background: var(--ca-surface-sunken); border-radius: 8px; padding: 12px 14px;
	}

	/* ─── Sign-in panel ──────────────────────────────────────────────── */
	.signin-panel {
		width: min(380px, 90%);
		display: flex; flex-direction: column; gap: 10px;
		align-items: flex-start;
	}
	.signin-title {
		font-size: 15px; font-weight: 600;
		color: var(--ca-text-primary); margin: 0;
	}
	.signin-hint {
		font-size: 12px; color: var(--ca-text-secondary); margin: 0 0 4px;
	}
	.signin-key-form {
		display: flex; flex-direction: column; gap: 10px; width: 100%;
	}
	.signin-terminal {
		width: 100%; display: flex; flex-direction: column;
		border: 1.5px solid var(--ca-border-subtle); border-radius: 8px; overflow: hidden;
	}
	.signin-term-output {
		background: var(--ca-surface-sunken); min-height: 160px; max-height: 260px;
		overflow-y: auto; padding: 10px 12px;
	}
	.signin-term-output pre {
		font-family: monospace; font-size: 11.5px; line-height: 1.6;
		color: var(--ca-text-primary); white-space: pre-wrap; word-break: break-all;
		margin: 0;
	}
	.signin-term-input-row {
		border-top: 1px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
	}
	.signin-term-input {
		width: 100%; box-sizing: border-box;
		padding: 8px 12px;
		background: none; border: none; outline: none;
		font-family: monospace; font-size: 12px; color: var(--ca-text-primary);
	}
</style>
