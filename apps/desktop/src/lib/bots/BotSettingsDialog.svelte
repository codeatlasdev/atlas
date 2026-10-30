<script lang="ts">
	/**
	 * BotSettingsDialog — large split-layout dialog replacing the profile drawer.
	 *
	 * Left column (~180px): section nav with bot face + name.
	 * Right column: scrollable content for the active section.
	 * All fields autosave: selects on change, text fields on blur / Cmd+Enter.
	 * Provider config keeps explicit "Save provider" + "Test" buttons.
	 */
	import { onMount } from "svelte";
	import type { Component } from "svelte";
	import { Dialog, Button, toast } from "@codeatlasdev/ui";
	import {
		Clock,
		FileText,
		Folder,
		GitBranch,
		GitPullRequest,
		Globe,
		Link,
		Bot as BotIcon,
		Pencil,
		Settings2,
		Timer,
		X,
		Zap,
	} from "@lucide/svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { b64encode } from "#lib/b64.ts";
	import {
		buildOmpModelsYml,
		buildClaudeEnvLines,
		buildCodexConfigToml,
		buildOpencodeConfig,
		OMP_PROVIDER_ENV,
		CODEX_PROVIDER_ENV,
		OPENCODE_PROVIDER_ENV,
		type ApiFormat,
		DEFAULT_SUMMARY_MODEL,
	} from "#lib/bots/providerConfig.ts";
	import { errText } from "#lib/api.ts";
	import { bots } from "#lib/stores/bots.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import type { BotProject, Trigger, TriggerKind } from "#lib/bots.ts";
	import BotEditor from "#lib/bots/BotEditor.svelte";
	import BotFace from "#lib/bots/BotFace.svelte";
	import { normalizeLook, type BotLook } from "#lib/bots/look.ts";
	import type { BotState as FaceState } from "#lib/bots/botFace.ts";
	import UsageSparkline from "#lib/bots/UsageSparkline.svelte";
	import ModelPicker from "#lib/bots/ModelPicker.svelte";
	import AutomationDialog from "#lib/bots/AutomationDialog.svelte";
	import { describeTrigger, describeNext } from "#lib/bots/automations.ts";
	import { projectForShip } from "#lib/bots/projectRef.ts";
	import { optionIn } from "#lib/bots/motion.ts";
	import { isDirty, updateSavedRef } from "#lib/bots/settings/autosave.ts";

	interface Props {
		open?: boolean;
	}

	let { open = $bindable(false) }: Props = $props();

	// ─── derived from stores ───────────────────────────────────────────────────

	const bot = $derived(bots.selectedBot);
	const ideProjects = $derived(
		ws.doc.worktrees.map((w) => ({ id: w.id, name: w.name, path: w.path, branch: w.branch ?? "" })),
	);
	const allTriggers = $derived(bot?.triggers ?? []);
	const faceState = $derived<FaceState>(
		bot?.state === "needs_you" ? "needs_you" : bot?.state === "working" ? "working" : "idle",
	);

	const AGENT_LABELS: Record<string, string> = {
		omp: "omp",
		claude: "Claude Code",
		codex: "Codex",
		opencode: "OpenCode",
	};
	const agentLabel = $derived(
		bot ? (agents.list.find((a) => a.id === bot.agent)?.label ?? AGENT_LABELS[bot.agent] ?? bot.agent) : "",
	);
	const serverName = $derived(bots.selected?.hostName ?? "");

	// ─── section nav ──────────────────────────────────────────────────────────

	const PROVIDER_AGENTS = ["omp", "claude", "codex", "opencode"];

	type SectionId = "general" | "automations" | "engine" | "memory" | "usage";

	const sections = $derived([
		{ id: "general" as SectionId, label: "General" },
		{ id: "automations" as SectionId, label: "Automations" },
		...(PROVIDER_AGENTS.includes(bot?.agent ?? "") ? [{ id: "engine" as SectionId, label: "Engine" }] : []),
		{ id: "memory" as SectionId, label: "Memory" },
		{ id: "usage" as SectionId, label: "Usage" },
	]);

	let activeSection = $state<SectionId>("general");
	let navItemEls: Array<HTMLButtonElement | null> = [];

	function navKeydown(e: KeyboardEvent, idx: number) {
		if (e.key === "ArrowDown") {
			e.preventDefault();
			navItemEls[Math.min(idx + 1, sections.length - 1)]?.focus();
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			navItemEls[Math.max(idx - 1, 0)]?.focus();
		}
	}

	// Opening put the focus (and its ring) on the face, the first focusable: start on the current section instead, so
	// ↑↓ move through the list right away.
	$effect(() => {
		if (!open) return;
		const i = sections.findIndex((s) => s.id === activeSection);
		requestAnimationFrame(() => navItemEls[i]?.focus({ focusVisible: false } as FocusOptions));
	});

	// ─── save-status helper ────────────────────────────────────────────────────

	type SaveStatus = "idle" | "saving" | "saved" | "error";

	function showSaved(set: (s: SaveStatus) => void) {
		set("saved");
		setTimeout(() => set("idle"), 1500);
	}

	// ─── general: role ─────────────────────────────────────────────────────────

	let settingsRole = $state("");
	let roleSavedRef = $state("");
	let roleStatus = $state<SaveStatus>("idle");
	let roleError = $state("");

	// ─── general: approval ────────────────────────────────────────────────────

	let settingsApproval = $state<"ask" | "auto">("auto");
	let approvalStatus = $state<SaveStatus>("idle");
	let approvalError = $state("");

	// ─── general: short replies ───────────────────────────────────────────────

	let settingsShortReplies = $state<"crew" | "on" | "off">("crew");
	let shortRepliesStatus = $state<SaveStatus>("idle");
	let shortRepliesError = $state("");

	// ─── engine / provider ────────────────────────────────────────────────────

	let providerBaseUrl = $state("https://openrouter.ai/api/v1");
	let providerFormat = $state<ApiFormat>("openai");
	let providerApiKey = $state("");
	let providerModel = $state("");
	let providerTesting = $state(false);
	let providerTestResult = $state<{ ok: boolean; text: string } | null>(null);
	let providerSaving = $state(false);
	let providerSaveError = $state("");

	// ─── memory ───────────────────────────────────────────────────────────────

	let memoryText = $state("");
	let memorySavedRef = $state("");
	let memoryLoading = $state(false);
	let memoryStatus = $state<SaveStatus>("idle");
	let memoryError = $state("");

	// ─── usage ────────────────────────────────────────────────────────────────

	let usageData = $state<number[]>([]);
	let usageTotals = $state({ today: 0, week: 0, month: 0 });

	// ─── automations ──────────────────────────────────────────────────────────

	let automationDialogOpen = $state(false);
	let automationEditTrigger = $state<Trigger | null>(null);

	// ─── look editor ──────────────────────────────────────────────────────────

	let editingLook = $state<BotLook | null>(null);
	let lookEditorOpen = $state(false);

	// ─── time for describeNext ────────────────────────────────────────────────

	let now = $state(Date.now());

	onMount(() => {
		const timer = setInterval(() => {
			now = Date.now();
		}, 1000);
		return () => clearInterval(timer);
	});

	// ─── initialise on open ───────────────────────────────────────────────────

	let prevOpen = false;
	$effect(() => {
		if (open && !prevOpen && bot) {
			activeSection = "general";
			// General fields
			settingsRole = bot.role ?? "";
			roleSavedRef = bot.role ?? "";
			roleStatus = "idle";
			roleError = "";
			settingsApproval = bot.approval;
			approvalStatus = "idle";
			approvalError = "";
			settingsShortReplies = bot.short_replies === true ? "on" : bot.short_replies === false ? "off" : "crew";
			shortRepliesStatus = "idle";
			shortRepliesError = "";
			// Provider fields — user fills in; no init from bot record
			providerBaseUrl = "https://openrouter.ai/api/v1";
			providerFormat = "openai";
			providerApiKey = "";
			providerModel = "";
			providerTestResult = null;
			providerSaveError = "";
			// Memory
			memoryText = "";
			memorySavedRef = "";
			memoryStatus = "idle";
			memoryError = "";
			// Usage
			usageData = [];
			usageTotals = { today: 0, week: 0, month: 0 };
			// Load data
			void loadMemory();
			void loadUsage();
		}
		prevOpen = open;
	});

	// ─── helpers ──────────────────────────────────────────────────────────────

	function fmtTokens(n: number): string {
		if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
		return String(n);
	}

	// ─── save: role ───────────────────────────────────────────────────────────

	async function saveRole() {
		if (!bots.selected || !bot || !isDirty(settingsRole, roleSavedRef)) return;
		roleStatus = "saving";
		roleError = "";
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { role: settingsRole });
			roleSavedRef = updateSavedRef(roleSavedRef, settingsRole, true);
			showSaved((s) => (roleStatus = s));
		} catch (e) {
			roleError = errText(e);
			roleStatus = "error";
		}
	}

	// ─── save: approval ───────────────────────────────────────────────────────

	async function saveApproval() {
		if (!bots.selected || !bot) return;
		approvalStatus = "saving";
		approvalError = "";
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { approval: settingsApproval });
			showSaved((s) => (approvalStatus = s));
		} catch (e) {
			approvalError = errText(e);
			approvalStatus = "error";
		}
	}

	// ─── save: short replies ──────────────────────────────────────────────────

	async function saveShortReplies() {
		if (!bots.selected || !bot) return;
		shortRepliesStatus = "saving";
		shortRepliesError = "";
		try {
			const val = settingsShortReplies === "on" ? true : settingsShortReplies === "off" ? false : null;
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			await bots.updateBot(bots.selected.hostKey, bot.id, { short_replies: val } as any);
			showSaved((s) => (shortRepliesStatus = s));
		} catch (e) {
			shortRepliesError = errText(e);
			shortRepliesStatus = "error";
		}
	}

	// ─── memory ───────────────────────────────────────────────────────────────

	async function loadMemory() {
		memoryLoading = true;
		try {
			const text = await bots.getMemory();
			memoryText = text;
			memorySavedRef = text;
		} catch {
			/* keep stale */
		}
		memoryLoading = false;
	}

	async function saveMemory() {
		if (!isDirty(memoryText, memorySavedRef)) return;
		memoryStatus = "saving";
		memoryError = "";
		try {
			await bots.setMemory(memoryText);
			memorySavedRef = updateSavedRef(memorySavedRef, memoryText, true);
			showSaved((s) => (memoryStatus = s));
		} catch (e) {
			memoryError = errText(e);
			memoryStatus = "error";
		}
	}

	// ─── usage ────────────────────────────────────────────────────────────────

	async function loadUsage() {
		try {
			const { records } = await bots.getUsage();
			const now2 = Date.now();
			const todayStart = new Date();
			todayStart.setHours(0, 0, 0, 0);
			const weekStart = now2 - 7 * 24 * 60 * 60 * 1000;
			const monthStart = now2 - 30 * 24 * 60 * 60 * 1000;
			const buckets = new Array(30).fill(0);
			for (const rec of records) {
				const tsMs = rec.ts * 1000;
				const dayIdx = Math.floor((now2 - tsMs) / (24 * 60 * 60 * 1000));
				if (dayIdx >= 0 && dayIdx < 30)
					buckets[29 - dayIdx] += (rec.input_tokens ?? 0) + (rec.output_tokens ?? 0);
			}
			usageData = buckets;
			usageTotals = {
				today: records
					.filter((r) => r.ts * 1000 >= todayStart.getTime())
					.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
				week: records
					.filter((r) => r.ts * 1000 >= weekStart)
					.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
				month: records
					.filter((r) => r.ts * 1000 >= monthStart)
					.reduce((s, r) => s + (r.input_tokens ?? 0) + (r.output_tokens ?? 0), 0),
			};
		} catch {
			/* keep stale */
		}
	}

	// ─── provider ─────────────────────────────────────────────────────────────

	async function testProvider() {
		if (!providerBaseUrl.trim() || !providerApiKey.trim()) return;
		providerTesting = true;
		providerTestResult = null;
		try {
			const models = await invoke<string[]>("provider_probe", {
				baseUrl: providerBaseUrl.trim(),
				apiKey: providerApiKey.trim(),
			});
			providerTestResult = {
				ok: true,
				text: models.length
					? `Connected — ${models.length} model${models.length === 1 ? "" : "s"}`
					: "Connected",
			};
		} catch (e) {
			providerTestResult = { ok: false, text: errText(e) };
		} finally {
			providerTesting = false;
		}
	}

	/**
	 * Saves provider config to the remote host and runs a one-shot connection check.
	 * Keeps explicit Save + Test buttons (not autosave) because writing files to the
	 * remote host and verifying the connection is a high-latency, side-effecting operation.
	 */
	async function saveProvider() {
		const hostKey = bots.selected?.hostKey;
		const agentId = bot?.agent;
		if (!hostKey || !agentId) return;
		if (!providerBaseUrl.trim() || !providerModel.trim() || !providerApiKey.trim()) return;
		providerSaving = true;
		providerSaveError = "";
		const sh = (script: string) =>
			invoke("host_call", {
				cmd: "exec",
				args: { host: hostKey, cwd: "/", argv: ["sh", "-c", script], timeout_ms: 30000 },
			});
		const dir = "/var/lib/atlas-bot/.atlas/agent-env";
		const baseUrl = providerBaseUrl.trim();
		const model = providerModel.trim();
		const key = providerApiKey.trim();
		const writeFile = (path: string, content: string) =>
			sh(
				`mkdir -p "$(dirname "${path}")"; ` +
					`printf '%s' '${btoa(content)}' | base64 -d > "${path}"; ` +
					`chmod 600 "${path}"`,
			);
		try {
			await sh(`umask 077; mkdir -p '${dir}' && chmod 700 '${dir}'`);
			if (agentId === "omp") {
				await writeFile(
					"$HOME/.omp/agent/models.yml",
					buildOmpModelsYml({
						baseUrl,
						apiFormat: providerFormat,
						model,
						summaryModel: DEFAULT_SUMMARY_MODEL,
					}),
				);
				await invoke("host_call", {
					cmd: "fs_write",
					args: { host: hostKey, path: `${dir}/omp.env`, data: b64encode(`${OMP_PROVIDER_ENV}=${key}\n`) },
				});
				await sh(`chmod 600 '${dir}/omp.env'`);
			} else if (agentId === "claude") {
				await invoke("host_call", {
					cmd: "fs_write",
					args: {
						host: hostKey,
						path: `${dir}/claude.env`,
						data: b64encode(buildClaudeEnvLines({ baseUrl, apiKey: key })),
					},
				});
				await sh(`chmod 600 '${dir}/claude.env'`);
			} else if (agentId === "codex") {
				await writeFile("$HOME/.codex/config.toml", buildCodexConfigToml({ baseUrl, model }));
				await invoke("host_call", {
					cmd: "fs_write",
					args: {
						host: hostKey,
						path: `${dir}/codex.env`,
						data: b64encode(`${CODEX_PROVIDER_ENV}=${key}\n`),
					},
				});
				await sh(`chmod 600 '${dir}/codex.env'`);
			} else if (agentId === "opencode") {
				await writeFile(
					"$HOME/.config/opencode/opencode.json",
					buildOpencodeConfig({ baseUrl, apiFormat: providerFormat, model, apiKey: key }),
				);
				await invoke("host_call", {
					cmd: "fs_write",
					args: {
						host: hostKey,
						path: `${dir}/opencode.env`,
						data: b64encode(`${OPENCODE_PROVIDER_ENV}=${key}\n`),
					},
				});
				await sh(`chmod 600 '${dir}/opencode.env'`);
			}
			// One-shot connection check
			const pathExport = `export PATH="$HOME/.bun/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$HOME/.opencode/bin:$PATH"`;
			const srcEnv = `ef="${dir}/${agentId}.env"; [ -f "$ef" ] && . "$ef" 2>/dev/null || true`;
			const checkCmds: Record<string, string> = {
				omp: `omp -p "reply with: ok" --model atlas_gateway/${model} 2>&1 | head -c 400`,
				claude: `claude -p "reply with: ok" 2>&1 | head -c 400`,
				codex: `codex exec --dangerously-bypass-hook-trust "Reply with only the word: ok" 2>&1 | head -c 400`,
				opencode: `opencode run --format json --standalone --auto "Reply with only the word: ok" 2>&1 | head -c 400`,
			};
			if (checkCmds[agentId]) {
				await invoke("host_call", {
					cmd: "exec",
					args: {
						host: hostKey,
						cwd: "/tmp",
						argv: ["sh", "-c", `${pathExport}; ${srcEnv}; ${checkCmds[agentId]}`],
						timeout_ms: 90000,
					},
				});
			}
			providerApiKey = "";
			toast.success("Provider configured");
		} catch (e) {
			providerSaveError = errText(e);
		} finally {
			providerSaving = false;
		}
	}

	// ─── trigger helpers ──────────────────────────────────────────────────────

	const TRIGGER_ICONS: Record<TriggerKind, Component> = {
		schedule: Clock,
		interval: Timer,
		webhook: Link,
		github: GitPullRequest,
		file: FileText,
		bot: BotIcon,
		service: Settings2,
	};

	function triggerIconComponent(kind: TriggerKind | string): Component {
		return (TRIGGER_ICONS as Record<string, Component>)[kind] ?? Zap;
	}

	async function toggleTrigger(id: string, enabled: boolean) {
		const updated = allTriggers.map((t) => (t.id === id ? { ...t, enabled } : t));
		try {
			await bots.setTriggers(updated);
		} catch (e) {
			toast.error(errText(e));
		}
	}

	function openNewAutomation() {
		automationEditTrigger = null;
		automationDialogOpen = true;
	}

	function openEditAutomation(t: Trigger) {
		automationEditTrigger = t;
		automationDialogOpen = true;
	}

	async function saveAutomation(trig: Trigger) {
		const list = allTriggers.some((t) => t.id === trig.id)
			? allTriggers.map((t) => (t.id === trig.id ? trig : t))
			: [...allTriggers, trig];
		try {
			await bots.setTriggers(list);
			automationDialogOpen = false;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function deleteAutomation(id: string) {
		try {
			await bots.setTriggers(allTriggers.filter((t) => t.id !== id));
			automationDialogOpen = false;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	// ─── project ──────────────────────────────────────────────────────────────

	async function linkProject(p: { id: string; name: string; path: string; branch: string }) {
		if (!bots.selected || !bot) return;
		const origin = await invoke<{ url: string | null }>("project_git_info", { path: p.path })
			.then((i) => i.url)
			.catch(() => null);
		const r = projectForShip(p, origin, bots.selected.hostKey);
		if (!r.ok) {
			toast.error(r.error);
			return;
		}
		const project: BotProject = r.project;
		await bots.updateBot(bots.selected.hostKey, bot.id, { project }).catch((e) => toast.error(errText(e)));
	}

	async function unlinkProject() {
		if (!bots.selected || !bot) return;
		await bots
			.updateBot(bots.selected.hostKey, bot.id, { project: null })
			.catch((e) => toast.error(errText(e)));
	}

	// ─── delete ───────────────────────────────────────────────────────────────

	async function deleteBot() {
		if (!bots.selected || !bot) return;
		if (!confirm(`Delete "${bot.name}"? This cannot be undone.`)) return;
		try {
			await bots.deleteBot(bots.selected.hostKey, bot.id);
			open = false;
		} catch (e) {
			toast.error(errText(e));
		}
	}

	// ─── look ─────────────────────────────────────────────────────────────────

	async function saveLook(newLook: BotLook) {
		if (!bots.selected || !bot) return;
		try {
			await bots.updateBot(bots.selected.hostKey, bot.id, { look: newLook });
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

{#if bot}
	<!-- Chromeless: the kit header was a bar above both columns with the name a second time; the nav runs full height
	     and the pane carries its own title and close. -->
	<Dialog
		bind:open
		title="{bot.name} settings"
		chromeless
		size="split"
		splitColumns="200px 1fr"
		splitSize={{ width: "min(820px, 92vw)", height: "min(600px, 85vh)" }}
		class="bot-settings"
	>
		<!-- ── Left: section nav ──────────────────────────────────────────── -->
		<!-- `closing`: the kit Dialog leaves the page only when every animation inside it has ended, and the face here
		     breathes and blinks forever, so a closed dialog stayed with an invisible backdrop that swallowed every click
		     (the same bug BotEditor had). Closing turns the animations off so the exit can finish. -->
		<nav class="settings-nav" class:closing={!open} aria-label="Settings sections">
			<div class="nav-top">
				<!-- The face is the "Customize look" button (a pencil shows on hover); no separate row for it. -->
				<button
					class="nav-avatar"
					aria-label="Customize look"
					title="Customize look"
					onclick={() => {
						editingLook = normalizeLook(bot.look, bot.id);
						lookEditorOpen = true;
					}}
				>
					<BotFace seed={bot.id} look={bot.look} state={faceState} size={64} track />
					<span class="nav-avatar-edit" aria-hidden="true"><Pencil size={11} /></span>
				</button>
				<span class="nav-bot-name">{bot.name}</span>
				{#if agentLabel}
					<span class="nav-bot-sub" title={serverName}
						>{agentLabel}{serverName ? ` · ${serverName.replace(/^atlas-bot@/, "")}` : ""}</span
					>
				{/if}
			</div>

			<ul class="nav-list" role="list">
				{#each sections as s, i (s.id)}
					<li>
						<button
							class="nav-item"
							class:active={activeSection === s.id}
							aria-current={activeSection === s.id ? "page" : undefined}
							bind:this={navItemEls[i]}
							onclick={() => (activeSection = s.id)}
							onkeydown={(e) => navKeydown(e, i)}>{s.label}</button
						>
					</li>
				{/each}
			</ul>

			<div class="nav-bottom">
				<button class="nav-delete" onclick={() => void deleteBot()}>Delete bot</button>
			</div>
		</nav>

		<!-- ── Right: content pane ────────────────────────────────────────── -->
		<div
			class="settings-content"
			class:closing={!open}
			role="region"
			aria-label={sections.find((s) => s.id === activeSection)?.label ?? "Settings"}
		>
			<header class="pane-head">
				<h3>{sections.find((s) => s.id === activeSection)?.label ?? "Settings"}</h3>
				<button class="pane-close" aria-label="Close" onclick={() => (open = false)}><X size={16} /></button>
			</header>
			<div class="settings-pane">
				{#if activeSection === "general"}
					<!-- ── General ─────────────────────────────────────────── -->

					<!-- What it does -->
					<section class="card">
						<div class="card-head">
							<span class="row-label">What it does</span>
							<span
								class="save-status"
								class:saving={roleStatus === "saving"}
								class:saved={roleStatus === "saved"}
								class:error={roleStatus === "error"}
							>
								{#if roleStatus === "saving"}Saving…{:else if roleStatus === "saved"}Saved{:else if roleStatus === "error"}{roleError}{/if}
							</span>
						</div>
						<p class="row-hint">Its job in a sentence or two. {bot.name} reads it before every task.</p>
						<textarea
							class="field-textarea"
							bind:value={settingsRole}
							rows={4}
							maxlength={2000}
							aria-label="What this bot does"
							onblur={() => void saveRole()}
							onkeydown={(e) => {
								if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
									e.preventDefault();
									void saveRole();
								}
							}}></textarea>
					</section>

					<!-- Behaviour: one row per setting, words on the left, the control on the right. -->
					<section class="card rows">
						<div class="row">
							<div class="row-text">
								<span class="row-label"
									>Approval
									<span
										class="save-status"
										class:saving={approvalStatus === "saving"}
										class:saved={approvalStatus === "saved"}
										class:error={approvalStatus === "error"}
										>{#if approvalStatus === "saving"}Saving…{:else if approvalStatus === "saved"}Saved{:else if approvalStatus === "error"}{approvalError}{/if}</span
									></span
								>
								<span class="row-hint">
									{bot.approval_supported === false
										? "OpenCode bots run without approval prompts."
										: "Whether it asks you before running commands or changing files."}
								</span>
							</div>
							<select
								class="field-select compact"
								bind:value={settingsApproval}
								disabled={bot.approval_supported === false}
								aria-label="Approval mode"
								onchange={() => void saveApproval()}
							>
								<option value="ask">Ask before acting</option>
								<option value="auto">Automatic</option>
							</select>
						</div>
						<div class="row">
							<div class="row-text">
								<span class="row-label"
									>Short replies
									<span
										class="save-status"
										class:saving={shortRepliesStatus === "saving"}
										class:saved={shortRepliesStatus === "saved"}
										class:error={shortRepliesStatus === "error"}
										>{#if shortRepliesStatus === "saving"}Saving…{:else if shortRepliesStatus === "saved"}Saved{:else if shortRepliesStatus === "error"}{shortRepliesError}{/if}</span
									></span
								>
								<span class="row-hint">Summarize long replies with a cheap model before showing them.</span>
							</div>
							<select
								class="field-select compact"
								bind:value={settingsShortReplies}
								aria-label="Short replies mode"
								onchange={() => void saveShortReplies()}
							>
								<option value="crew">Crew setting</option>
								<option value="on">Always</option>
								<option value="off">Never</option>
							</select>
						</div>
					</section>

					<!-- A bot has one project: a row here, not a page of its own. -->
					<section class="card rows">
						<div class="row">
							<span class="auto-icon" aria-hidden="true">
								{#if bot.project?.kind === "path"}<Folder size={15} />{:else}<GitBranch size={15} />{/if}
							</span>
							<div class="row-text auto-text">
								<span class="row-label">Project</span>
								{#if bot.project}
									<span class="row-hint auto-prompt" title={bot.project.url ?? bot.project.path}
										>{(bot.project.url ?? bot.project.path ?? bot.project.name ?? "").replace(
											/^https:\/\/github\.com\/|\.git$/g,
											"",
										)}{bot.project.branch ? ` · ${bot.project.branch}` : ""}</span
									>
								{:else}
									<span class="row-hint">Not linked. Pick one of your projects:</span>
								{/if}
							</div>
							{#if bot.project}
								<Button size="sm" variant="ghost" onclick={() => void unlinkProject()}>Unlink</Button>
							{/if}
						</div>
						{#if !bot.project}
							<div class="project-options">
								{#each ideProjects as p (p.id)}
									<button class="project-option" onclick={() => void linkProject(p)}>
										<GitBranch size={12} aria-hidden="true" />
										{p.name}
										{#if p.branch}<span class="muted">{p.branch}</span>{/if}
									</button>
								{:else}
									<p class="row-hint">No projects open in Atlas yet.</p>
								{/each}
							</div>
						{/if}
					</section>
				{:else if activeSection === "automations"}
					<!-- ── Automations ──────────────────────────────────── -->
					{#if allTriggers.length > 0}
						<section class="card rows">
							{#each allTriggers as trig, i (trig.id)}
								{@const TrigIcon = triggerIconComponent(trig.kind.type as TriggerKind)}
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="row automation-row"
									role="button"
									tabindex="0"
									aria-label="Edit automation"
									onclick={() => openEditAutomation(trig)}
									onkeydown={(e) => e.key === "Enter" && openEditAutomation(trig)}
									in:optionIn={{ index: i }}
								>
									<span class="auto-icon" aria-hidden="true"><TrigIcon size={15} /></span>
									<div class="row-text auto-text">
										<span class="row-label">{describeTrigger(trig.kind, trig.tz)}</span>
										{#if trig.prompt}<span class="row-hint auto-prompt">{trig.prompt}</span>{/if}
										{#if trig.next_fire && trig.enabled}
											<span class="row-hint">Next: {describeNext(trig.next_fire, now)}</span>
										{/if}
									</div>
									<!-- A switch, not a checkbox: on/off is the one thing to do from the row. -->
									<!-- svelte-ignore a11y_click_events_have_key_events -->
									<input
										class="switch"
										type="checkbox"
										role="switch"
										checked={trig.enabled}
										aria-label="Automation on"
										onclick={(e) => e.stopPropagation()}
										onchange={(e) =>
											void toggleTrigger(trig.id, (e.currentTarget as HTMLInputElement).checked)}
									/>
								</div>
							{/each}
						</section>
					{:else}
						<section class="card empty-card">
							<span class="row-label">No automations yet</span>
							<p class="row-hint">
								Add one below, or just ask {bot.name} in the chat: “every weekday at 9, review the open PRs”.
							</p>
						</section>
					{/if}
					<div class="pane-actions">
						<Button size="sm" variant="outlined" onclick={openNewAutomation}>Add automation</Button>
					</div>
				{:else if activeSection === "engine"}
					<!-- ── Engine ───────────────────────────────────────── -->
					<section class="card rows">
						<div class="row">
							<div class="row-text">
								<span class="row-label">Engine</span>
								<span class="row-hint">The coding agent that runs {bot.name}'s tasks.</span>
							</div>
							<span class="row-value">{agentLabel}</span>
						</div>
					</section>

					<!-- The provider is one unit (files written on the bot's computer + a connection check), so it keeps an
					     explicit Save and Test instead of saving field by field. -->
					<section class="card">
						<div class="card-head">
							<span class="row-label"><Globe size={13} aria-hidden="true" /> Custom provider</span>
						</div>
						<p class="row-hint">Optional. Route this engine through your own API gateway.</p>
						<div class="form-grid">
							<label for="bsd-prov-url">Base URL</label>
							<input
								id="bsd-prov-url"
								class="field-input"
								type="url"
								placeholder="https://openrouter.ai/api/v1"
								bind:value={providerBaseUrl}
							/>
							{#if bot.agent !== "claude"}
								<label for="bsd-prov-fmt">API format</label>
								<select id="bsd-prov-fmt" class="field-select" bind:value={providerFormat}>
									<option value="openai">OpenAI-compatible</option>
									<option value="anthropic">Anthropic-compatible</option>
								</select>
							{/if}
							<label for="bsd-prov-key">API key</label>
							<input
								id="bsd-prov-key"
								class="field-input"
								type="password"
								placeholder="sk-…"
								bind:value={providerApiKey}
							/>
							<span class="form-label">Model</span>
							<ModelPicker baseUrl={providerBaseUrl} apiKey={providerApiKey} bind:value={providerModel} />
						</div>
						{#if providerTestResult}
							<p class="provider-result" class:ok={providerTestResult.ok}>{providerTestResult.text}</p>
						{/if}
						{#if providerSaveError}
							<p class="provider-save-error" role="alert">{providerSaveError}</p>
						{/if}
						<div class="pane-actions">
							<Button
								size="sm"
								variant="ghost"
								disabled={providerTesting || !providerBaseUrl.trim() || !providerApiKey.trim()}
								onclick={() => void testProvider()}
							>
								{providerTesting ? "Testing…" : "Test connection"}
							</Button>
							<Button
								size="sm"
								variant="outlined"
								disabled={providerSaving ||
									!providerBaseUrl.trim() ||
									!providerModel.trim() ||
									!providerApiKey.trim()}
								onclick={() => void saveProvider()}
							>
								{providerSaving ? "Saving…" : "Save provider"}
							</Button>
						</div>
					</section>
				{:else if activeSection === "memory"}
					<!-- ── Memory ──────────────────────────────────────── -->
					{#if memoryLoading}
						<div class="loading">Loading…</div>
					{:else}
						<!-- What the bot keeps between tasks: its own notes, readable text that fills the pane. -->
						<section class="card memory-card">
							<div class="card-head">
								<span class="row-label">What {bot.name} remembers</span>
								<span
									class="save-status"
									class:saving={memoryStatus === "saving"}
									class:saved={memoryStatus === "saved"}
									class:error={memoryStatus === "error"}
									>{#if memoryStatus === "saving"}Saving…{:else if memoryStatus === "saved"}Saved{:else if memoryStatus === "error"}{memoryError}{/if}</span
								>
							</div>
							<p class="row-hint">
								Notes {bot.name} writes for itself and reads before every task. Edit or remove anything that is
								wrong.
							</p>
							<textarea
								class="field-textarea memory-textarea"
								bind:value={memoryText}
								placeholder="Nothing yet. It learns as it works."
								aria-label="Bot memory"
								onblur={() => void saveMemory()}
								onkeydown={(e) => {
									if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
										e.preventDefault();
										void saveMemory();
									}
								}}></textarea>
						</section>
					{/if}
				{:else}
					<!-- ── Usage ───────────────────────────────────────── -->
					<UsageSparkline data={usageData} />
					<div class="usage-totals">
						<span class="usage-stat">Today: {fmtTokens(usageTotals.today)}</span>
						<span class="usage-stat">7d: {fmtTokens(usageTotals.week)}</span>
						<span class="usage-stat">30d: {fmtTokens(usageTotals.month)}</span>
					</div>
				{/if}
			</div>
		</div>
	</Dialog>

	<!-- Outside <Dialog> to avoid animation trapping (same pattern as BotEditor in BotView). -->
	<BotEditor
		bind:open={lookEditorOpen}
		look={editingLook ?? normalizeLook(bot.look, bot.id)}
		seed={bot.id}
		onsave={saveLook}
	/>
	<AutomationDialog
		bind:open={automationDialogOpen}
		trigger={automationEditTrigger}
		botName={bot.name}
		tz={Intl.DateTimeFormat().resolvedOptions().timeZone}
		onsave={saveAutomation}
		ondelete={deleteAutomation}
	/>
{/if}

<style>
	.closing,
	.closing :global(*) {
		animation: none !important;
		transition: none !important;
	}
	/* ── Nav column ─────────────────────────────────────────────────────── */
	.settings-nav {
		display: flex;
		flex-direction: column;
		border-right: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		overflow-y: auto;
	}

	.nav-top {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 5px;
		padding: 28px 12px 16px;
	}

	.nav-avatar {
		position: relative;
		padding: 0;
		border: 0;
		background: none;
		cursor: pointer;
		border-radius: 50%;
		margin-bottom: 4px;
	}
	/* Always shown: hidden until hover, nothing said the face could be customized. */
	.nav-avatar-edit {
		position: absolute;
		right: -2px;
		bottom: -2px;
		display: inline-grid;
		place-items: center;
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: var(--ca-surface-raised, var(--ca-surface-base));
		border: 1px solid var(--ca-outline-subtle);
		color: var(--ca-text-primary);
		box-shadow: 0 1px 3px rgb(0 0 0 / 0.35);
		transition:
			background 120ms,
			color 120ms;
	}
	.nav-avatar::after {
		content: "";
		position: absolute;
		inset: -4px;
		border-radius: 50%;
		border: 2px solid transparent;
		transition: border-color 120ms;
		pointer-events: none;
	}
	.nav-avatar:hover::after {
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, transparent);
	}
	.nav-avatar:hover .nav-avatar-edit,
	.nav-avatar:focus-visible .nav-avatar-edit {
		background: var(--ca-brand-main);
		color: var(--ca-text-on-brand);
	}
	.nav-avatar:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: 2px;
	}

	.nav-bot-name {
		font-size: 13px;
		font-weight: 600;
		text-align: center;
		color: var(--ca-text-primary);
	}

	.nav-bot-sub {
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: center;
		max-width: 100%;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.nav-list {
		list-style: none;
		margin: 0;
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 1px;
		flex: 1;
	}

	.nav-item {
		display: block;
		width: 100%;
		text-align: left;
		font: inherit;
		font-size: 13px;
		padding: 7px 10px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
		transition:
			background 80ms,
			color 80ms;
	}
	.nav-item:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.nav-item.active,
	.nav-item[aria-current="page"] {
		background: color-mix(in oklab, var(--ca-brand-main) 12%, var(--ca-surface-base));
		color: var(--ca-text-primary);
		font-weight: 500;
	}
	.nav-item:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: -2px;
	}

	.nav-bottom {
		padding: 8px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		margin-top: auto;
	}

	.nav-delete {
		display: block;
		width: 100%;
		text-align: left;
		font: inherit;
		font-size: 12px;
		padding: 6px 10px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--ca-semantic-error);
		cursor: default;
	}
	.nav-delete:hover {
		background: color-mix(in oklab, var(--ca-semantic-error) 10%, transparent);
	}

	/* ── Content column ─────────────────────────────────────────────────── */
	/* The pane head stays put; only the fields below it scroll. */

	/* min-width: 0 — a grid item defaults to its content's width, and a long automation prompt (nowrap) pushed the
	   pane past the dialog's right edge. */
	.settings-content {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	/* The kit's overlay surface is a lighter blue-grey than the app; use the app's own surfaces (sidebar sunken, pane
	   base), like the chat behind it. */
	:global(dialog.ca-dialog.bot-settings) {
		background: var(--ca-surface-base);
	}
	.pane-head {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 18px 16px 6px 28px;
	}
	.pane-head h3 {
		margin: 0;
		font-size: 17px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.pane-close {
		display: inline-grid;
		place-items: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.pane-close:hover {
		background: var(--ca-vibrancy-fill);
		color: var(--ca-text-primary);
	}
	.settings-pane {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 14px 28px 28px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	/* ── Field groups ───────────────────────────────────────────────────── */

	/* ── Save status ────────────────────────────────────────────────────── */
	.save-status {
		font-size: 11px;
		opacity: 0;
		transition: opacity 200ms;
		pointer-events: none;
	}
	.save-status.saving,
	.save-status.saved,
	.save-status.error {
		opacity: 1;
	}
	.save-status.saving {
		color: var(--ca-text-secondary);
	}
	.save-status.saved {
		color: var(--ca-semantic-success, #22c55e);
	}
	.save-status.error {
		color: var(--ca-semantic-error, #ef4444);
		pointer-events: auto;
	}

	/* ── Inputs ─────────────────────────────────────────────────────────── */
	.field-textarea {
		width: 100%;
		font-size: 13px;
		padding: 9px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		resize: vertical;
		box-sizing: border-box;
		line-height: 1.4;
		font-family: inherit;
	}
	.field-textarea:focus {
		outline: none;
		border-color: var(--ca-action-focus, var(--ca-brand-main));
	}

	.memory-card {
		flex: 1;
		min-height: 280px;
	}
	.memory-textarea {
		flex: 1;
		resize: none;
		font-size: 13px;
		line-height: 1.6;
		padding: 12px 14px;
	}

	.field-select {
		appearance: none;
		font: inherit;
		font-size: 13px;
		height: 36px;
		padding: 0 32px 0 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken)
			url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%239aa3b5' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")
			no-repeat right 12px center;
		color: var(--ca-text-primary);
		width: 100%;
		box-sizing: border-box;
		cursor: pointer;
	}
	.field-select:focus {
		outline: none;
		border-color: var(--ca-action-focus, var(--ca-brand-main));
	}
	.field-select:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.field-input {
		width: 100%;
		font: inherit;
		font-size: 13px;
		padding: 8px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		box-sizing: border-box;
	}
	.field-input:focus {
		outline: none;
		border-color: var(--ca-action-focus, var(--ca-brand-main));
	}

	/* ── Cards and rows (macOS Settings style) ─────────────────────────── */
	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 14px 16px;
		border-radius: 12px;
		background: color-mix(in oklab, var(--ca-text-primary) 3%, transparent);
		border: 0.5px solid var(--ca-outline-subtle);
	}
	.card.rows {
		padding: 0 16px;
		gap: 0;
	}
	.card-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 24px;
		padding: 14px 0;
	}
	.row + .row {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.row-text {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.row-label {
		font-size: 13px;
		font-weight: 500;
		color: var(--ca-text-primary);
		display: inline-flex;
		align-items: baseline;
		gap: 8px;
	}
	.row-hint {
		font-size: 12px;
		line-height: 1.4;
		color: var(--ca-text-secondary);
		margin: 0;
	}
	.field-select.compact {
		width: auto;
		min-width: 160px;
		flex: none;
	}

	/* ── Engine ─────────────────────────────────────────────────────────── */
	.row-value {
		font-size: 13px;
		color: var(--ca-text-secondary);
		flex: none;
	}
	.form-grid {
		display: grid;
		grid-template-columns: 96px 1fr;
		align-items: center;
		gap: 10px 14px;
		margin-top: 6px;
	}
	.form-grid > label,
	.form-label {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}

	.provider-result {
		font-size: 12px;
		margin: 0;
		color: var(--ca-semantic-error, #ef4444);
	}
	.provider-result.ok {
		color: var(--ca-semantic-success, #22c55e);
	}

	.provider-save-error {
		font-size: 12px;
		margin: 0;
		color: var(--ca-semantic-error, #ef4444);
	}

	.pane-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	/* ── Automations ────────────────────────────────────────────────────── */
	.automation-row {
		justify-content: flex-start;
		gap: 12px;
		margin: 0 -16px;
		padding: 14px 16px;
		cursor: default;
		transition: background 80ms;
	}
	.automation-row:hover {
		background: var(--ca-vibrancy-fill);
	}
	.automation-row:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: -2px;
	}
	.card.rows .automation-row:first-child {
		border-top-left-radius: 12px;
		border-top-right-radius: 12px;
	}
	.card.rows .automation-row:last-child {
		border-bottom-left-radius: 12px;
		border-bottom-right-radius: 12px;
	}
	.auto-icon {
		flex: none;
		display: inline-grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
		color: var(--ca-brand-main);
	}
	.auto-text {
		flex: 1;
	}
	.auto-prompt {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.empty-card {
		align-items: flex-start;
	}

	/* iOS-style switch on a native checkbox (keeps keyboard and screen-reader behaviour). */
	.switch {
		appearance: none;
		flex: none;
		position: relative;
		width: 34px;
		height: 20px;
		margin: 0;
		border-radius: 999px;
		background: color-mix(in oklab, var(--ca-text-primary) 18%, transparent);
		cursor: default;
		transition: background 140ms;
	}
	.switch::after {
		content: "";
		position: absolute;
		top: 2px;
		left: 2px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		box-shadow: 0 1px 2px rgb(0 0 0 / 0.3);
		transition: transform 140ms;
	}
	.switch:checked {
		background: var(--ca-semantic-success);
	}
	.switch:checked::after {
		transform: translateX(14px);
	}
	.switch:focus-visible {
		outline: 2px solid var(--ca-focus-ring);
		outline-offset: 2px;
	}

	/* ── Project ────────────────────────────────────────────────────────── */

	.project-options {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.project-option {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		border: none;
		background: none;
		border-radius: 6px;
		text-align: left;
		font-size: 12.5px;
		color: var(--ca-text-primary);
		cursor: default;
	}
	.project-option:hover {
		background: var(--ca-vibrancy-fill);
	}

	.muted {
		color: var(--ca-text-secondary);
		font-size: 11px;
	}

	/* ── Usage ──────────────────────────────────────────────────────────── */
	.usage-totals {
		display: flex;
		gap: 12px;
		padding: 4px 0;
	}

	.usage-stat {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}

	/* ── Loading ────────────────────────────────────────────────────────── */
	.loading {
		color: var(--ca-text-secondary);
		font-size: 14px;
	}
</style>
