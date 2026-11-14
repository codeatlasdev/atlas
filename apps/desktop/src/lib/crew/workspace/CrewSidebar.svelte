<script lang="ts">
	/**
	 * CrewSidebar — the Crew workspace's left column: header, the first mate (hero), the crewmate list grouped by crew
	 * server and project, crew-server failure rows and the footer. It also publishes the on-screen order for ⌘1…⌘9.
	 */
	import { flip } from "svelte/animate";
	import { crewServerLabel, isCrewServerKey } from "#lib/crew/sshTarget.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { shortcutOrder } from "#lib/crew/crewmateOrder.ts";
	import {
		favoriteOf,
		groupSections,
		loadFavorites,
		saveFavorites,
		visibleSections,
		withFavorite,
	} from "#lib/crew/sidebar.ts";
	import { handleContextAction } from "#lib/crew/workspace/actions.svelte.ts";
	import CrewmateListRow from "#lib/crew/CrewmateListRow.svelte";
	import HeroFavorite from "#lib/crew/workspace/HeroFavorite.svelte";
	import SidebarHeader from "#lib/crew/workspace/SidebarHeader.svelte";
	import SidebarFooter from "#lib/crew/workspace/SidebarFooter.svelte";

	let {
		hidden = false,
		onintegrations,
		onsettings,
		onhelp,
	}: { hidden?: boolean; onintegrations: () => void; onsettings: () => void; onhelp: () => void } = $props();

	let favMap = $state(loadFavorites());

	const favoriteEntry = $derived(favoriteOf(favMap, crew.allCrewmates));
	const nonFavCrewmates = $derived(
		favoriteEntry
			? crew.allCrewmates.filter(
					(x) => !(x.hostKey === favoriteEntry.hostKey && x.crewmate.id === favoriteEntry.crewmate.id),
				)
			: crew.allCrewmates,
	);
	const sections = $derived(groupSections(crew.allCrewmates, crew.servers));
	const listed = $derived(visibleSections(sections, nonFavCrewmates));

	// ⌘1…⌘9 follow what is on screen: the first mate, then the list (commands.svelte.ts reads it from the store).
	$effect(() => {
		crew.sidebarOrder = shortcutOrder(favoriteEntry, sections);
	});

	function setFavorite(hostKey: string, crewmateId: string) {
		favMap = withFavorite(favMap, hostKey, crewmateId);
		saveFavorites(favMap);
	}

	function select(hostKey: string, crewmateId: string) {
		void crew.select(hostKey, crewmateId);
	}

	function isActive(hostKey: string, crewmateId: string): boolean {
		return crew.selected?.hostKey === hostKey && crew.selected?.crewmateId === crewmateId;
	}
</script>

<!-- Hidden (not removed: scroll and list state survive) while the Board is up: it covers every crewmate,
     so picking one there meant nothing. -->
<div class="col-left" {hidden}>
	<SidebarHeader />

	{#if favoriteEntry}
		<HeroFavorite
			hero={favoriteEntry}
			active={isActive(favoriteEntry.hostKey, favoriteEntry.crewmate.id)}
			onselect={() => select(favoriteEntry.hostKey, favoriteEntry.crewmate.id)}
		/>
	{/if}

	<!-- Crewmate list (non-favorite). With no crewmates the footer "Add a crew server" is the CTA; with only the hero
	     there is nothing more to list. -->
	<div class="crewmate-list" role="list">
		{#each crew.servers.filter((h) => !h.ok && isCrewServerKey(h.key)) as h (h.key)}
			<!-- A crew server that didn't answer: say so instead of an empty list that looks like the crew is gone. -->
			<div class="empty-state" role="status">
				<p class="empty-sub">
					Can't reach {crewServerLabel(h.name)}{h.error ? `: ${h.error}` : ""}
				</p>
				<button class="text-link" onclick={() => void crew.retryFailed()}>Retry</button>
			</div>
		{/each}
		{#each listed as section (section.hostKey)}
			{#if sections.length > 1}
				<div class="host-hd">{section.hostName}</div>
			{/if}
			{#each section.projects as pg (pg.projectName ?? "__none__")}
				{#if pg.projectName}
					<div class="project-hd"><span>{pg.projectName}</span></div>
				{/if}
				{#each pg.crewmates as { hostKey, crewmate } (crewmate.id)}
					<div animate:flip={{ duration: 200 }}>
						<CrewmateListRow
							{crewmate}
							active={isActive(hostKey, crewmate.id)}
							unread={crew.hasUnread(hostKey, crewmate.id)}
							isFavorite={favoriteEntry?.hostKey === hostKey && favoriteEntry?.crewmate.id === crewmate.id}
							onclick={() => select(hostKey, crewmate.id)}
							onFavorite={() => setFavorite(hostKey, crewmate.id)}
							onContextAction={(action) => handleContextAction(hostKey, crewmate.id, action)}
						/>
					</div>
				{/each}
			{/each}
		{/each}
	</div>

	<SidebarFooter {onintegrations} {onsettings} {onhelp} />
</div>

<style>
	.col-left[hidden] {
		display: none;
	}
	.col-left {
		width: 270px;
		flex: none;
		display: flex;
		flex-direction: column;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
	}

	/* Host header (multi-host only) */
	.host-hd {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
		padding: 10px 12px 2px;
	}

	/* Project sub-header within a host section */
	.project-hd {
		font-size: 10.5px;
		font-weight: 600;
		letter-spacing: 0.05em;
		color: var(--ca-text-secondary);
		padding: 8px 12px 2px;
		text-transform: uppercase;
	}

	/* Crewmate list */
	.crewmate-list {
		flex: 1;
		overflow-y: auto;
	}

	/* Empty states */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 32px 16px;
		gap: 6px;
		text-align: center;
	}

	.empty-sub {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}
</style>
