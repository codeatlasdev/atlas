/**
 * The Atlas Crew onboarding flow as one `$state` class: the step machine, the crew-server install (which key or
 * password signs in, the install stream), the engine checks/installs, the sign-in terminal, the API key and
 * own-provider forms and the final "open chat". The components only render it and call its methods; everything that
 * talks to Tauri/hostd lives here (engine config scripts live in `remoteAgent.ts`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { errText } from "#lib/api.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { builtinAgent, isBuiltinAgent } from "#lib/agentRegistry.ts";
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
import { AGENT_AUTH_TYPE, applyAgentApiKey } from "#lib/crew/remoteAgent.ts";
import type { SshHost } from "#lib/crew/sshHosts.ts";
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
	completeInstall,
	initialInstallProgress,
	type InstallProgress,
} from "./installProgress.ts";
import type { InstallPhase } from "#lib/crew/installSteps.ts";
import {
	describeConnectError,
	folderOf,
	parseConnectError,
	privateKeyPath,
	type ConnectAction,
	type ConnectContext,
	type ConnectFailure,
	type KeyLookup,
} from "./connectError.ts";
import { IS_WIN } from "#lib/keys.ts";

/** How the form signs in to the server: the SSH key of this computer, or the account password (once, to add the key). */
export type SignInMethod = "key" | "password";

/** The footer's primary button (see `Onboarding.primary`). */
export type Primary = { label: string; run: () => void; busy: boolean; disabled: boolean; reason: string };

type InstallLineEvent = { line: string; is_err: boolean };

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
	/** Opt-in: also install the capture browsers (`atlas crew install --browsers`). Off for a first install. */
	browsers = $state(false);
	/** The user's `~/.ssh/config` servers, for the picker above the address field. */
	sshHosts = $state<SshHost[]>([]);
	install = $state<InstallProgress>(initialInstallProgress());
	/** Phase that failed (shown in red). */
	failedPhase = $state<InstallPhase | null>(null);
	/** What the last failure means and what to do next, or null. */
	failure = $state<ConnectFailure | null>(null);
	installRunning = $state(false);
	installDone = $state(false);
	/** Host key of the crew server launched in this onboarding; the first crewmate is created there. */
	launchedNow = $state("");
	fieldShake = $state(false);
	/** Sign in with the key (the default when there is one) or, once, with the account password. */
	method = $state<SignInMethod>("key");
	private methodChosen = false;
	/** The account password of the SSH user: held while the form is open, sent once, never stored. */
	sshPassword = $state("");
	/** The private key file the user picked; "" lets ssh find it (`~/.ssh/config`, the default names). */
	identity = $state("");
	/** The key ssh will sign in with for the address typed, or null when there is none. */
	key = $state<KeyLookup["key"]>(null);
	readonly hasKey = $derived(this.key !== null);
	/** `ssh` exists on this computer (on Windows it is an optional feature). */
	sshFound = $state(true);
	keyChecked = $state(false);
	creatingKey = $state(false);
	/** The sudo password, when the server's sudo asks for one. */
	sudoPassword = $state("");
	/** The address this run added the key to with the password: a refused key after that is not "not authorized yet". */
	private addedFor = $state("");
	private started = false;
	private keySeq = 0;
	private keyQuery = "";
	/** One spelling of where the form points, to tell when it moved. */
	private readonly where = $derived(`${this.sshTarget.trim()}:${this.sshPort}`);
	readonly keyAdded = $derived(this.addedFor !== "" && this.addedFor === this.where);
	/** The sudo password is asked below the form. */
	readonly needsSudo = $derived(this.failure?.actions.includes("sudo") ?? false);
	/**
	 * The crew server this onboarding works on: the one launched here, else the one already connected (the connect
	 * step is skipped when a crew server exists, which left this empty and the engine checks stuck on "Checking…").
	 */
	readonly launchedKey = $derived(
		this.launchedNow || crew.servers.find((h) => isCrewServerKey(h.key))?.key || "",
	);
	readonly targetValid = $derived(
		this.sshTarget.trim() === "" ? false : parseSSHTarget(`${this.sshTarget.trim()}:${this.sshPort}`).ok,
	);
	/** The form is complete: a valid address, ssh present, and the key or the password to sign in with. */
	readonly canSubmit = $derived(
		this.targetValid &&
			this.sshFound &&
			!this.installRunning &&
			!this.installDone &&
			(this.method === "key" ? this.key !== null : this.sshPassword !== ""),
	);

	/**
	 * The ONE primary action of the footer, for whatever step and state the flow is in. The footer never moves: only
	 * this label, its busy spinner and `disabled` change, and `reason` is what the content says when it is disabled.
	 */
	readonly primary = $derived.by((): Primary => {
		const go = (label: string, run: () => void, extra: Partial<Primary> = {}): Primary => ({
			label,
			run,
			busy: false,
			disabled: false,
			reason: "",
			...extra,
		});
		switch (this.step) {
			case "connect":
				if (this.needsSudo) {
					return go(
						this.installRunning ? "Installing…" : "Continue",
						() => void this.launchServer(this.sudoPassword),
						{
							busy: this.installRunning,
							disabled: this.installRunning || !this.sudoPassword,
						},
					);
				}
				return go(
					this.installRunning
						? this.install.done.has("done")
							? "Connecting…"
							: "Installing…"
						: "Add crew server",
					() => void this.launchServer(),
					{
						busy: this.installRunning,
						disabled: !this.canSubmit,
					},
				);
			case "engine":
				if (this.signinActive) return go("Back to engines", () => this.closeSignin());
				if (this.readyEngine) return go("Continue", () => this.continueWithReadyEngine());
				return go("Continue", () => {}, {
					busy: this.installingAny,
					disabled: true,
					reason: "Click an engine to install it on the server and sign in.",
				});
			case "ready":
				return go("Open chat", () => void this.openChat(), {
					busy: this.creatingCrewmate,
					disabled: this.creatingCrewmate || !this.engineReady,
					reason:
						this.engineReady || this.creatingCrewmate
							? ""
							: this.selectedEngine
								? `${this.selectedEngine} isn't ready on your crew server yet (${this.engineStatus[this.selectedEngine]?.phase ?? "not checked"}). Go back and finish it.`
								: "Go back and pick an engine.",
				});
			default:
				return go("Continue", this.advance);
		}
	});

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

	readonly integrations = $derived(agents.list.filter((a) => isBuiltinAgent(a.id)));
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

	/**
	 * Look for the SSH key once the connect step is showing, and list the servers of `~/.ssh/config` for the picker
	 * (no config, or one that cannot be read, is an empty list and the address field stays on its own).
	 */
	checkSshKey() {
		if (this.step !== "connect" || this.started) return;
		this.started = true;
		void invoke<SshHost[]>("ssh_hosts_list")
			.then((h) => (this.sshHosts = h))
			.catch(() => {});
		void this.resolveKey();
	}

	/** What `ssh_key_resolve` is asked: the address typed (a `~/.ssh/config` alias has its own key) and the picked file. */
	private keyQueryNow() {
		const t = parseSSHTarget(this.where);
		return {
			target: t.ok ? t.target : null,
			port: t.ok && t.port !== 22 ? t.port : null,
			identity: this.identity || null,
		};
	}

	/** Ask which key ssh will use. The first answer also decides the method: the key when there is one, else the password (which creates it). */
	async resolveKey() {
		const seq = ++this.keySeq;
		const query = this.keyQueryNow();
		this.keyQuery = JSON.stringify(query);
		const r = await invoke<KeyLookup>("ssh_key_resolve", query).catch(() => null);
		if (seq !== this.keySeq) return; // a newer question is on its way: this answer is about an old address
		this.sshFound = r?.ssh_found ?? true;
		this.key = r?.key ?? null;
		this.keyChecked = true;
		if (!this.methodChosen) this.method = this.key ? "key" : "password";
	}

	/** Re-ask when the address, the port or the picked key changes (waits for the typing to pause). Returns the cleanup. */
	watchKey(): (() => void) | undefined {
		if (this.step !== "connect" || !this.started) return;
		const timer = setTimeout(() => {
			if (JSON.stringify(this.keyQueryNow()) !== this.keyQuery) void this.resolveKey();
		}, 350);
		return () => clearTimeout(timer);
	}

	/** Fill the address and port from a server of the user's `~/.ssh/config`. */
	pickSshHost(host: SshHost) {
		this.sshTarget = host.address;
		this.sshPort = host.port;
	}

	chooseMethod(method: SignInMethod) {
		this.method = method;
		this.methodChosen = true;
		this.failure = null;
	}

	/** "Choose another key": the OS file picker, opened in the folder of the key shown. */
	async pickKeyFile() {
		const picked = await pickFile({
			title: "Choose a private SSH key",
			multiple: false,
			directory: false,
			defaultPath: folderOf(this.key?.identity ?? ""),
		});
		if (typeof picked !== "string") return;
		this.identity = privateKeyPath(picked);
		this.chooseMethod("key");
		await this.resolveKey();
	}

	/** Back to the key ssh finds by itself. */
	useDefaultKey() {
		this.identity = "";
		void this.resolveKey();
	}

	async createSshKey() {
		this.creatingKey = true;
		this.failure = null;
		try {
			await invoke("ssh_keygen");
			await this.resolveKey();
		} catch (e) {
			this.failure = describeConnectError(parseConnectError(e), this.failureContext());
		} finally {
			this.creatingKey = false;
		}
	}

	private failureContext(): ConnectContext {
		return {
			keyPath: this.key?.identity ?? null,
			keyAdded: this.keyAdded,
			target: this.sshTarget.trim(),
			windows: IS_WIN,
		};
	}

	/** The button of a failure block. */
	runAction(action: ConnectAction) {
		if (action === "pick_key") void this.pickKeyFile();
		else if (action === "use_password") this.chooseMethod("password");
		else if (action === "use_key") this.chooseMethod("key");
		else if (action === "recheck") void this.resolveKey();
		else if (action === "retry") void this.launchServer();
	}

	private shakeField() {
		this.fieldShake = true;
		setTimeout(() => (this.fieldShake = false), 500);
	}

	/**
	 * Sign in, install, connect. Signing in is the first checklist step: with a key, one connection that also
	 * trusts a new server's host key; with the password, the account password adds this computer's key to the
	 * server once (the method then becomes the key, so a retry does not ask again). Accepts an optional sudo password
	 * (piped to the CLI's stdin, never argv).
	 */
	async launchServer(sudoPassword?: string) {
		const target = this.sshTarget.trim();
		if (!target) {
			this.shakeField();
			return;
		}
		this.installRunning = true;
		this.installDone = false;
		this.failedPhase = null;
		this.failure = null;
		this.install = { ...initialInstallProgress(), active: "signin" };

		const unlisten = await listen<InstallLineEvent>("crew://install-line", (e) => {
			this.install = applyInstallLine(this.install, e.payload.line);
		});

		try {
			const port = this.sshPort !== 22 ? this.sshPort : null;
			// The key shown is the key used: pass it explicitly (a `~/.ssh/config` with its own IdentityFile hides the
			// default names from ssh, so a key Atlas just created would never be offered otherwise).
			let identity = this.identity || this.key?.identity || null;
			if (this.method === "password" && !this.keyAdded) {
				const added = await invoke<KeyLookup["key"]>("ssh_authorize_key", {
					target,
					port,
					identity: this.identity || null,
					password: this.sshPassword,
				});
				if (added?.identity) {
					this.key = added;
					identity = added.identity;
				}
				this.sshPassword = "";
				this.addedFor = this.where;
				// The key is how this server is reached from now on, whatever a later lookup says.
				this.chooseMethod("key");
				void this.resolveKey(); // a key may have been created
			} else {
				await invoke("ssh_connect_check", { target, port, identity });
			}
			const args: Record<string, unknown> = { target, port, browsers: this.browsers, identity };
			if (sudoPassword !== undefined) args.sudoPassword = sudoPassword;
			await invoke("crew_server_install", args);
			// hostd runs as the service user the installer reported (a server that kept `atlas-bot` keeps it).
			const server = crewServerTarget(target, this.install.user || undefined);
			// Keep the address (and the key picked for it) the install ran against: the browsers' status and install
			// need an account that can sudo, and every later connection must use the same key.
			await crew.addServer(server, server, {
				target,
				...(port ? { port } : {}),
				...(identity ? { identity } : {}),
			});
			this.launchedNow = `ssh:${server}`;
			this.install = completeInstall(this.install);
			this.installDone = true;
			this.sudoPassword = "";
			setTimeout(this.advance, 900);
		} catch (e) {
			this.failedPhase = this.install.active;
			this.failure = describeConnectError(parseConnectError(e), this.failureContext());
			this.shakeField();
		} finally {
			unlisten();
			this.installRunning = false;
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
			.addServer(server, server, {
				target: old.key.slice(4),
				...(old.admin?.identity ? { identity: old.admin.identity } : {}),
			})
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
		if (!this.launchedKey || this.signinPtyId) return;
		this.signinActive = agentId;
		this.apiKeyMode = false;
		this.signinOutput = "";
		this.signinInput = "";
		this.signinPtyId = null;
		try {
			await openSigninPty(
				this.launchedKey,
				builtinAgent(agentId)?.bin ?? agentId,
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
				this.apiKeyError = "Key set but agent does not recognize it yet. Try the interactive flow.";
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
