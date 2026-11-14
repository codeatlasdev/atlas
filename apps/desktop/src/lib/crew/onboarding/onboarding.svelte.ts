/**
 * The Atlas Crew onboarding flow as one `$state` class: the step machine, the crew-server install (SSH key, auth
 * flows, install stream), the engine checks/installs, the sign-in terminal, the API key and own-provider forms and
 * the final "open chat". The components only render it and call its methods; everything that talks to Tauri/hostd
 * lives here (engine config scripts live in `remoteAgent.ts`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { errText } from "#lib/api.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { SEEN_KEY } from "#lib/crew/crewOnboarding.svelte.ts";
import { randomLook } from "#lib/crew/look.ts";
import { ProviderForm } from "#lib/crew/providerForm.svelte.ts";
import { openSigninPty, watchSigninPty, writeSigninLine } from "./signinPty.ts";
import {
	INITIAL_STEP,
	infoStepIndex,
	nextStep,
	prevStep,
	stepProgress,
	type OnboardingStep,
} from "#lib/crew/onboardingFlow.ts";
import { crewServerTarget, isCrewServerKey, parseSSHTarget } from "#lib/crew/sshTarget.ts";
import { AGENT_AUTH_TYPE, AGENT_SIGNIN_CMD, applyAgentApiKey } from "#lib/crew/remoteAgent.ts";
import {
	phaseFromCheck,
	pickReadyEngine,
	statusAfterInstall,
	STEP_META,
	type AgentCheck,
	type EngineStatus,
} from "./engines.ts";
import {
	applyInstallLine,
	authModeForError,
	completeInstall,
	initialInstallProgress,
	type InstallProgress,
} from "./installProgress.ts";
import type { InstallPhase } from "#lib/crew/installSteps.ts";

type InstallLineEvent = { line: string; is_err: boolean };

/** Engines the onboarding offers (ids = the agent registry). */
const ENGINE_IDS = ["claude", "opencode", "codex", "omp"];

export class Onboarding {
	// ─── step state ───────────────────────────────────────────────────────────
	step = $state<OnboardingStep>(INITIAL_STEP);
	direction = $state(1);
	/** Which line of the how-it-works story the art is on (set by CrewArtHow). */
	howBeat = $state(0);
	readonly hasHost = $derived(crew.servers.some((h) => h.key !== ""));
	readonly progress = $derived(stepProgress({ step: this.step, hasHost: this.hasHost }));
	readonly isInfoStep = $derived(infoStepIndex(this.step) >= 0);
	readonly meta = $derived(STEP_META[this.step]);

	// ─── connect step ─────────────────────────────────────────────────────────
	sshTarget = $state("");
	sshPort = $state(22);
	install = $state<InstallProgress>(initialInstallProgress());
	/** Phase that failed (shown in red). */
	failedPhase = $state<InstallPhase | null>(null);
	/** Last error line from stderr. */
	lastErrLine = $state("");
	installRunning = $state(false);
	installDone = $state(false);
	/** Host key of the crew server launched in this onboarding; the first crewmate is created there. */
	launchedNow = $state("");
	fieldShake = $state(false);
	keyChecked = $state(false);
	hasKey = $state(false);
	creatingKey = $state(false);
	keyError = $state("");
	/** "authorize" = Permission denied → copy key via password. "sudo" = sudo needs password. */
	authMode = $state<"none" | "authorize" | "sudo">("none");
	authPassword = $state("");
	authRunning = $state(false);
	authError = $state("");

	/**
	 * The crew server this onboarding works on: the one launched here, else the one already connected (the connect
	 * step is skipped when a crew server exists, which left this empty and the engine checks stuck on "Checking…").
	 */
	readonly launchedKey = $derived(
		this.launchedNow || crew.servers.find((h) => isCrewServerKey(h.key))?.key || "",
	);
	readonly serverSshTarget = $derived(this.launchedKey ? this.launchedKey.replace(/^ssh:/, "") : "");
	readonly targetValid = $derived(
		this.sshTarget.trim() === "" ? false : parseSSHTarget(`${this.sshTarget.trim()}:${this.sshPort}`).ok,
	);

	// ─── engine step ──────────────────────────────────────────────────────────
	engineStatus = $state<Record<string, EngineStatus>>({});
	installLog = $state(""); // accumulated engine install stdout/stderr
	signinActive = $state<string | null>(null); // agent being signed in
	signinPtyId = $state<string | null>(null);
	signinOutput = $state(""); // accumulated PTY output (ANSI stripped)
	signinInput = $state("");
	apiKeyValue = $state("");
	apiKeyApplying = $state(false);
	apiKeyError = $state("");
	apiKeyMode = $state(false);
	readonly provider = new ProviderForm();
	/** Whether the provider form is showing instead of the key/terminal. */
	providerOpen = $state(false);
	selectedEngine = $state("");

	readonly integrations = $derived(agents.list.filter((a) => ENGINE_IDS.includes(a.id)));
	readonly engineReady = $derived(
		this.selectedEngine !== "" && this.engineStatus[this.selectedEngine]?.phase === "ready",
	);
	readonly readyEngine = $derived(pickReadyEngine(this.selectedEngine, this.integrations, this.engineStatus));
	readonly readyEngineLabel = $derived(
		this.integrations.find((a) => a.id === this.readyEngine)?.label ?? this.readyEngine,
	);
	readonly installingAny = $derived(Object.values(this.engineStatus).some((s) => s.phase === "installing"));

	// ─── ready step ───────────────────────────────────────────────────────────
	creatingCrewmate = $state(false);
	readonly previewLook = randomLook();

	private promoting = false;

	constructor(private readonly ondone: () => void) {}

	// ─── navigation ───────────────────────────────────────────────────────────

	advance = () => {
		this.direction = 1;
		const next = nextStep({ step: this.step, hasHost: this.hasHost });
		if (next === "done") this.finish();
		else this.step = next;
	};

	back = () => {
		this.direction = -1;
		const prev = prevStep({ step: this.step, hasHost: this.hasHost });
		if (prev) this.step = prev;
	};

	finish = () => {
		try {
			localStorage.setItem(SEEN_KEY, "1");
		} catch {
			/* private browsing */
		}
		this.ondone();
	};

	// ─── connect step ─────────────────────────────────────────────────────────

	/** Detect the SSH key once the connect step is showing. */
	checkSshKey() {
		if (this.step !== "connect" || this.keyChecked) return;
		void invoke<string | null>("ssh_key_find").then((p) => {
			this.hasKey = p !== null;
			this.keyChecked = true;
		});
	}

	async createSshKey() {
		this.creatingKey = true;
		this.keyError = "";
		try {
			await invoke("ssh_keygen");
			this.hasKey = true;
		} catch (e) {
			this.keyError = errText(e);
		} finally {
			this.creatingKey = false;
		}
	}

	private shakeField() {
		this.fieldShake = true;
		setTimeout(() => (this.fieldShake = false), 500);
	}

	cancelAuth() {
		this.authMode = "none";
		this.authPassword = "";
	}

	/** Core install runner. Accepts an optional sudo password (piped securely). */
	async launchServer(sudoPassword?: string) {
		const target = this.sshTarget.trim();
		if (!target) {
			this.shakeField();
			return;
		}
		this.installRunning = true;
		this.installDone = false;
		this.failedPhase = null;
		this.lastErrLine = "";
		this.install = initialInstallProgress();
		this.authMode = "none";
		this.authError = "";

		const unlisten = await listen<InstallLineEvent>("crew://install-line", (e) => {
			this.install = applyInstallLine(this.install, e.payload.line);
		});

		try {
			const args: Record<string, unknown> = { target, port: this.sshPort !== 22 ? this.sshPort : null };
			if (sudoPassword !== undefined) args.sudoPassword = sudoPassword;
			await invoke("crew_server_install", args);
			// hostd runs as the service user the installer reported (a server that kept `atlas-bot` keeps it).
			const server = crewServerTarget(target, this.install.user || undefined);
			await crew.addServer(server, server);
			this.launchedNow = `ssh:${server}`;
			this.install = completeInstall(this.install);
			this.installDone = true;
			setTimeout(this.advance, 900);
		} catch (e) {
			const err = errText(e);
			this.failedPhase = this.install.active;
			this.lastErrLine = err;
			const mode = authModeForError(err);
			if (mode !== "none") this.authMode = mode;
			this.shakeField();
		} finally {
			unlisten();
			this.installRunning = false;
		}
	}

	/** Authorize this Mac's key on the server via password, then retry install. */
	async authorizeAndRetry() {
		this.authRunning = true;
		this.authError = "";
		const pw = this.authPassword;
		this.authPassword = ""; // clear from state immediately
		try {
			await invoke("ssh_authorize_key", {
				target: this.sshTarget.trim(),
				port: this.sshPort !== 22 ? this.sshPort : null,
				password: pw,
			});
			this.authMode = "none";
			await this.launchServer();
		} catch (e) {
			this.authError = errText(e);
			this.authMode = "authorize"; // stay in auth mode so user can retry
		} finally {
			this.authRunning = false;
		}
	}

	/**
	 * A crew server registered under the admin user (`root@host`, before the service-user fix) has no crewmate hostd
	 * for that user: register the service-user side of the same server and work there ("ssh:root@… is not connected"
	 * on the engine step).
	 */
	promoteOldServer() {
		if (this.step !== "engine" || this.launchedKey || this.promoting) return;
		const old = crew.servers.find((h) => h.key.startsWith("ssh:"));
		if (!old) return;
		this.promoting = true;
		const server = crewServerTarget(old.key.slice(4));
		void crew
			.addServer(server, server)
			.then(() => (this.launchedNow = `ssh:${server}`))
			.catch((e) => toast.error(errText(e)))
			.finally(() => (this.promoting = false));
	}

	// ─── engine step: check, install ─────────────────────────────────────────

	private checkAgent(agentId: string, hostKey = this.launchedKey) {
		return invoke<AgentCheck>("crew_agent_check", { hostKey, agentId });
	}

	/** Check every engine on the crew server once the engine step is showing. */
	checkEngines() {
		if (this.step !== "engine" || !this.launchedKey) return;
		for (const agent of this.integrations) {
			const cur = this.engineStatus[agent.id];
			if (cur && cur.phase !== "idle") continue;
			this.engineStatus[agent.id] = { phase: "checking" };
			this.checkAgent(agent.id)
				.then((r) => {
					this.engineStatus[agent.id] = { phase: phaseFromCheck(r) };
				})
				.catch((e) => {
					this.engineStatus[agent.id] = { phase: "not_installed", error: errText(e) };
				});
		}
	}

	async installEngine(agentId: string) {
		this.engineStatus[agentId] = { phase: "installing" };
		this.installLog = "";
		const unlisten = await listen<InstallLineEvent>("crew://install-line", (e) => {
			this.installLog += (this.installLog ? "\n" : "") + e.payload.line;
		});
		try {
			await invoke("crew_agent_install", { hostKey: this.launchedKey, agentId });
			const r = await this.checkAgent(agentId);
			this.engineStatus[agentId] = statusAfterInstall(r);
			// Installed but not signed in: open its sign-in straight away, as if the user had clicked the card again.
			if (r.installed && !r.signed_in) this.handleEngineCardClick(agentId);
		} catch (e) {
			this.engineStatus[agentId] = { phase: "install_error", error: errText(e) };
		} finally {
			unlisten();
		}
	}

	handleEngineCardClick(agentId: string) {
		const status = this.engineStatus[agentId];
		if (!status) return;
		switch (status.phase) {
			case "not_installed":
			case "install_error":
				void this.installEngine(agentId);
				break;
			case "needs_signin":
				// Reset all sign-in state when switching engines.
				if (this.signinActive !== agentId) this.providerOpen = false;
				this.signinActive = agentId;
				this.signinPtyId = null;
				this.signinOutput = "";
				this.signinInput = "";
				this.apiKeyValue = "";
				this.apiKeyError = "";
				this.provider.resetFeedback();
				if (AGENT_AUTH_TYPE[agentId] === "apikey") {
					this.apiKeyMode = true;
					this.providerOpen = false;
				} else if (!this.providerOpen) {
					this.apiKeyMode = false;
					void this.openSignInTerminal(agentId);
				}
				break;
			case "ready":
				this.selectEngine(agentId);
				break;
			case "checking":
			case "installing":
				break;
		}
	}

	selectEngine(id: string) {
		this.selectedEngine = id;
		// Automatically advance to ready after a short hold so selection is visible
		setTimeout(this.advance, 560);
	}

	/** Continue with the engine that is Ready (after install/sign-in), no second click on the card. */
	continueWithReadyEngine() {
		this.selectedEngine = this.readyEngine;
		this.advance();
	}

	// ─── engine step: sign-in terminal ───────────────────────────────────────

	closeSignin() {
		this.signinActive = null;
		this.signinPtyId = null;
		this.apiKeyMode = false;
	}

	async openSignInTerminal(agentId: string) {
		if (!this.serverSshTarget || this.signinPtyId) return;
		this.signinActive = agentId;
		this.apiKeyMode = false;
		this.signinOutput = "";
		this.signinInput = "";
		this.signinPtyId = null;
		try {
			await openSigninPty(
				this.serverSshTarget,
				AGENT_SIGNIN_CMD[agentId] ?? agentId,
				(pty) => (this.signinPtyId = pty),
			);
		} catch (e) {
			this.signinOutput = errText(e);
		}
	}

	/** Stream the sign-in pty into `signinOutput` and re-check the engine when it exits. Returns the cleanup. */
	watchSigninPty(): (() => void) | undefined {
		if (!this.signinPtyId) return;
		return watchSigninPty(
			this.signinPtyId,
			(text) => (this.signinOutput += text),
			() => void this.recheckSignin(),
		);
	}

	async recheckSignin() {
		const agentId = this.signinActive;
		if (!agentId || !this.launchedKey) return;
		const r = await this.checkAgent(agentId).catch(() => ({ installed: true, signed_in: false }));
		if (r.signed_in) {
			this.engineStatus[agentId] = { phase: "ready" };
			this.signinPtyId = null;
			this.signinActive = null;
		}
	}

	async sendSigninKey(e: KeyboardEvent) {
		if (!this.signinPtyId || e.key !== "Enter") return;
		await writeSigninLine(this.signinPtyId, this.signinInput);
		this.signinInput = "";
	}

	// ─── engine step: API key and own provider ───────────────────────────────

	toggleApiKeyMode() {
		this.apiKeyMode = !this.apiKeyMode;
	}

	openProviderMode() {
		this.providerOpen = true;
		this.apiKeyMode = false;
		this.provider.resetFeedback();
	}

	closeProviderMode() {
		this.providerOpen = false;
		if (AGENT_AUTH_TYPE[this.signinActive ?? ""] === "apikey") this.apiKeyMode = true;
	}

	async applyApiKey() {
		const agentId = this.signinActive;
		if (!this.apiKeyValue.trim() || !agentId || !this.launchedKey) return;
		this.apiKeyApplying = true;
		this.apiKeyError = "";
		try {
			await applyAgentApiKey(this.launchedKey, agentId, this.apiKeyValue);
			this.apiKeyValue = "";
			const r = await this.checkAgent(agentId);
			if (r.signed_in) {
				this.engineStatus[agentId] = { phase: "ready" };
				this.signinActive = null;
				this.apiKeyMode = false;
			} else {
				this.apiKeyError = "Key set but agent does not recognise it yet. Try the interactive flow.";
			}
		} catch (e) {
			this.apiKeyError = errText(e);
		} finally {
			this.apiKeyApplying = false;
		}
	}

	/** Write the engine's gateway config on the crew server, check it end to end, and mark the engine ready. */
	async saveProvider() {
		const agentId = this.signinActive;
		if (!agentId || !(await this.provider.save(this.launchedKey, agentId))) return;
		this.engineStatus[agentId] = { phase: "ready" };
		this.providerOpen = false;
		this.signinActive = null;
	}

	// ─── ready step ───────────────────────────────────────────────────────────

	async openChat() {
		if (this.creatingCrewmate) return;
		// The crew server launched here, else the first remote crew server; hosts[0] is this Mac, never the crew's home.
		const hostKey = this.launchedKey;
		this.creatingCrewmate = true;
		try {
			if (this.selectedEngine) {
				// Replaying the onboarding must not pile up crewmates: reuse the crew server's crewmate on this engine.
				// A reused crewmate that never finished setup will have setup_step set; it will resume setup in CrewmateView.
				const existing = crew.servers
					.find((h) => h.key === hostKey)
					?.crewmates.find((c) => c.agent === this.selectedEngine);
				const id =
					existing?.id ??
					(await crew.createCrewmate(hostKey, {
						name: "New crewmate",
						role: "",
						agent: this.selectedEngine,
						approval: "auto",
						look: this.previewLook,
						setup_step: "name",
					}));
				await crew.select(hostKey, id);
			}
			this.finish();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			this.creatingCrewmate = false;
		}
	}
}
