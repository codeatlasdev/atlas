<script lang="ts">
	/**
	 * CrewmateSettingsDialog: large split-layout dialog replacing the profile drawer. The shell owns the section nav,
	 * the pane head and the section switch; each section is its own component, state and saves live in
	 * settings.svelte.ts (autosaved fields) and crew/providerForm.svelte.ts (explicit Save / Test).
	 * All fields autosave: selects on change, text fields on blur / Cmd+Enter.
	 */
	import { Dialog, toast } from "@codeatlasdev/ui";
	import { X } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import CrewmateEditor from "#lib/crew/CrewmateEditor.svelte";
	import { normalizeLook, type CrewmateLook } from "#lib/crew/look.ts";
	import { CrewmateSettings } from "#lib/crew/settings/settings.svelte.ts";
	import { PROVIDER_AGENTS, ProviderForm } from "#lib/crew/providerForm.svelte.ts";
	import SettingsNav from "#lib/crew/settings/SettingsNav.svelte";
	import GeneralSection from "#lib/crew/settings/GeneralSection.svelte";
	import AutomationsSection from "#lib/crew/settings/AutomationsSection.svelte";
	import EngineSection from "#lib/crew/settings/EngineSection.svelte";
	import MemorySection from "#lib/crew/settings/MemorySection.svelte";
	import UsageSection from "#lib/crew/settings/UsageSection.svelte";
	import "#lib/crew/settings/settings.css";

	interface Props {
		open?: boolean;
	}

	let { open = $bindable(false) }: Props = $props();

	const crewmate = $derived(crew.selectedCrewmate);
	const settings = new CrewmateSettings();
	const provider = new ProviderForm();

	const AGENT_LABELS: Record<string, string> = {
		omp: "omp",
		claude: "Claude Code",
		codex: "Codex",
		opencode: "OpenCode",
	};
	const agentLabel = $derived(
		crewmate
			? (agents.list.find((a) => a.id === crewmate.agent)?.label ??
					AGENT_LABELS[crewmate.agent] ??
					crewmate.agent)
			: "",
	);
	const serverName = $derived(crew.selected?.hostName ?? "");

	// ─── section nav ──────────────────────────────────────────────────────────

	type SectionId = "general" | "automations" | "engine" | "memory" | "usage";

	const sections = $derived([
		{ id: "general" as SectionId, label: "General" },
		{ id: "automations" as SectionId, label: "Automations" },
		...(PROVIDER_AGENTS.includes(crewmate?.agent ?? "")
			? [{ id: "engine" as SectionId, label: "Engine" }]
			: []),
		{ id: "memory" as SectionId, label: "Memory" },
		{ id: "usage" as SectionId, label: "Usage" },
	]);
	let activeSection = $state<SectionId>("general");
	const activeLabel = $derived(sections.find((s) => s.id === activeSection)?.label ?? "Settings");
	let navItemEls = $state<Array<HTMLButtonElement | null>>([]);

	// Opening put the focus (and its ring) on the face, the first focusable: start on the current section instead, so
	// ↑↓ move through the list right away.
	$effect(() => {
		if (!open) return;
		const i = sections.findIndex((s) => s.id === activeSection);
		requestAnimationFrame(() => navItemEls[i]?.focus({ focusVisible: false } as FocusOptions));
	});

	// Every open starts from the crewmate's current values on the General section.
	let prevOpen = false;
	$effect(() => {
		if (open && !prevOpen && crewmate) {
			activeSection = "general";
			settings.open(crewmate);
			provider.reset();
		}
		prevOpen = open;
	});

	// ─── look + delete ────────────────────────────────────────────────────────

	let editingLook = $state<CrewmateLook | null>(null);
	let lookEditorOpen = $state(false);

	async function saveLook(newLook: CrewmateLook) {
		if (!crew.selected || !crewmate) return;
		try {
			await crew.updateCrewmate(crew.selected.hostKey, crewmate.id, { look: newLook });
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function deleteCrewmate() {
		if (!crew.selected || !crewmate) return;
		if (!confirm(`Delete "${crewmate.name}"? This cannot be undone.`)) return;
		try {
			await crew.deleteCrewmate(crew.selected.hostKey, crewmate.id);
			open = false;
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

{#if crewmate}
	<!-- Chromeless: the kit header was a bar above both columns with the name a second time; the nav runs full height
	     and the pane carries its own title and close. -->
	<Dialog
		bind:open
		title="{crewmate.name} settings"
		chromeless
		size="split"
		splitColumns="200px 1fr"
		splitSize={{ width: "min(820px, 92vw)", height: "min(600px, 85vh)" }}
		class="crewmate-settings"
	>
		<SettingsNav
			{crewmate}
			{sections}
			bind:active={activeSection}
			bind:itemEls={navItemEls}
			closing={!open}
			{agentLabel}
			{serverName}
			oneditlook={() => {
				editingLook = normalizeLook(crewmate.look, crewmate.id);
				lookEditorOpen = true;
			}}
			ondelete={() => void deleteCrewmate()}
		/>

		<div class="settings-content" class:closing={!open} role="region" aria-label={activeLabel}>
			<header class="pane-head">
				<h3>{activeLabel}</h3>
				<button class="pane-close" aria-label="Close" onclick={() => (open = false)}><X size={16} /></button>
			</header>
			<div class="settings-pane">
				{#if activeSection === "general"}
					<GeneralSection {crewmate} {settings} />
				{:else if activeSection === "automations"}
					<AutomationsSection {crewmate} {settings} />
				{:else if activeSection === "engine"}
					<EngineSection {crewmate} {agentLabel} {provider} />
				{:else if activeSection === "memory"}
					<MemorySection {crewmate} {settings} />
				{:else}
					<UsageSection {settings} />
				{/if}
			</div>
		</div>
	</Dialog>

	<!-- Outside <Dialog> to avoid animation trapping (same pattern as CrewmateEditor in CrewmateView). -->
	<CrewmateEditor
		bind:open={lookEditorOpen}
		look={editingLook ?? normalizeLook(crewmate.look, crewmate.id)}
		seed={crewmate.id}
		onsave={saveLook}
	/>
{/if}

<style>
	.closing,
	.closing :global(*) {
		animation: none !important;
		transition: none !important;
	}
	/* The pane head stays put; only the fields below it scroll. min-width: 0 — a grid item defaults to its
	   content's width, and a long automation prompt (nowrap) pushed the pane past the dialog's right edge. */
	.settings-content {
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
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
</style>
