<script lang="ts">
	/**
	 * CrewWorkspace — full-window messenger layout (Discord DM style).
	 * Left column: CrewSidebar. Right panel: the Board, the selected crewmate's chat, or the empty state.
	 * Also hosts the overlays: integrations catalog, onboarding and the ⌘K palette.
	 */
	import "#lib/crew/crew.css";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { palettes } from "#lib/shell/palettes.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import CrewmateView from "../../routes/(app)/CrewmateView.svelte";
	import { crewOnboarding, SEEN_KEY } from "#lib/crew/crewOnboarding.svelte.ts";
	import { githubCrew } from "#lib/crew/githubCrew.svelte.ts";
	import CrewOnboarding from "#lib/crew/onboarding/CrewOnboarding.svelte";
	import IntegrationsCatalog from "#lib/crew/IntegrationsCatalog.svelte";
	import CrewPalette from "#lib/crew/CrewPalette.svelte";
	import CrewViewBar from "#lib/crew/CrewViewBar.svelte";
	import CrewBoard from "#lib/crew/CrewBoard.svelte";
	import PauseBanner from "#lib/crew/PauseBanner.svelte";
	import { crewView } from "#lib/crew/crewView.svelte.ts";
	import CrewSidebar from "#lib/crew/workspace/CrewSidebar.svelte";
	import WorkspaceEmpty from "#lib/crew/workspace/WorkspaceEmpty.svelte";
	import { recruit } from "#lib/crew/workspace/actions.svelte.ts";

	let catalogOpen = $state(false);
	let catalogDetail = $state<string | null>(null);

	// First run: no crew yet and never seen. Replays (the "?" button, the palette, Settings) set the same shared flag.
	if (
		typeof localStorage !== "undefined" &&
		!localStorage.getItem(SEEN_KEY) &&
		crew.allCrewmates.length === 0
	)
		crewOnboarding.open = true;

	// React to external signal (e.g. from CrewmateView error button).
	$effect(() => {
		if (githubCrew.openIntegrations) {
			catalogOpen = true;
			catalogDetail = "github";
			githubCrew.openIntegrations = false;
		}
	});

	function openCatalog() {
		catalogOpen = true;
		catalogDetail = null;
	}
</script>

<div class="crewmates-workspace">
	<!-- Cartography grid overlay -->
	<svg class="crew-grid-bg" aria-hidden="true" xmlns="http://www.w3.org/2000/svg">
		<defs>
			<pattern id="crew-grid" width="40" height="40" patternUnits="userSpaceOnUse">
				<path d="M 40 0 L 0 0 0 40" fill="none" stroke="rgba(255,255,255,0.07)" stroke-width="0.5" />
			</pattern>
		</defs>
		<rect width="100%" height="100%" fill="url(#crew-grid)" />
	</svg>

	<CrewSidebar
		hidden={crewView.value === "board"}
		onintegrations={openCatalog}
		onsettings={() => updates.show("crew")}
		onhelp={() => (crewOnboarding.open = true)}
	/>

	<!-- Top-edge view switcher (board ↔ chat): only in Crew mode -->
	<CrewViewBar />

	<!-- Right panel -->
	<div class="col-right">
		<PauseBanner />
		{#if crewView.value === "board"}
			<CrewBoard />
		{:else if crew.selectedCrewmate}
			<CrewmateView />
		{:else}
			<WorkspaceEmpty />
		{/if}
	</div>

	<IntegrationsCatalog bind:open={catalogOpen} bind:detail={catalogDetail} />
	{#if crewOnboarding.open}
		<CrewOnboarding ondone={() => (crewOnboarding.open = false)} />
	{/if}
	<CrewPalette
		bind:open={palettes.crew}
		crewmates={crew.allCrewmates.map(({ hostKey, crewmate }) => ({
			id: crewmate.id,
			hostKey,
			name: crewmate.name,
			role: crewmate.role ?? "",
			look: crewmate.look,
			state: crewmate.state,
		}))}
		onclose={() => (palettes.crew = false)}
		onSelectCrewmate={(hk, id) => void crew.select(hk, id)}
		onNewCrewmate={() => void recruit.start()}
		onIntegrations={openCatalog}
		onServerSettings={() => updates.show("crew")}
		onHelp={() => (crewOnboarding.open = true)}
	/>
</div>

<style>
	.crewmates-workspace {
		position: relative;
		display: flex;
		width: 100%;
		height: 100%;
		background: var(--ca-surface-base);
		overflow: hidden;
	}

	/* ─── Right panel ─────────────────────────────────────────── */
	.col-right {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}

	/* ─── Crew grid background ────────────────────────────────── */
	.crew-grid-bg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		pointer-events: none;
		z-index: 0;
	}
	@media (prefers-color-scheme: light) {
		.crew-grid-bg path {
			stroke: rgba(0, 0, 0, 0.07);
		}
	}
</style>
