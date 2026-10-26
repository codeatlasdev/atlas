<script lang="ts">
	/**
	 * Settings dialog: section nav on the left, the selected page (or search results) on the right. What exists,
	 * its titles and its tabs all come from the registry (`lib/settings/registry.ts`); this file only lays it out.
	 */
	import { Dialog, IconButton, Tabs, TabsContent, TabsList, TabsTrigger } from "@codeatlasdev/ui";
	import { X } from "@lucide/svelte";
	import { tick } from "svelte";
	import {
		SECTIONS,
		SECTION_TABS,
		searchSettings,
		type SettingEntry,
		type SettingsTab,
	} from "#lib/settings/index.ts";
	import PageHead from "#lib/settings/PageHead.svelte";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import { PAGE_COMPONENTS, type PageKey } from "./settings/pageComponents.ts";
	import SearchResults from "./settings/SearchResults.svelte";
	import SettingsNav from "./settings/SettingsNav.svelte";

	let results = $derived(searchSettings(u.query));
	let searching = $derived(u.query.trim() !== "");
	let paneEl: HTMLDivElement | undefined = $state();

	let section = $derived(SECTIONS.find((s) => s.id === u.section)!);
	let tabs = $derived(SECTION_TABS[u.section]);
	/** A tab id that does not belong to the open section (left over from another) reads as its first tab. */
	let tab = $derived(tabs?.find((t) => t.id === u.tab) ?? tabs?.[0]);
	let Page = $derived(PAGE_COMPONENTS[(tab ? `${u.section}/${tab.id}` : u.section) as PageKey]);

	/** Scroll to the row a deep link or a search result names, and flash it once its page has rendered. */
	$effect(() => {
		const id = u.open && !searching ? u.reveal : null;
		if (!id) return;
		void (async () => {
			await tick();
			await new Promise((r) => requestAnimationFrame(r));
			const row = paneEl?.querySelector<HTMLElement>(`[data-setting="${id}"]`);
			u.reveal = null;
			if (!row) return;
			row.scrollIntoView({ block: "center", behavior: "smooth" });
			row.classList.add("flash");
			setTimeout(() => row.classList.remove("flash"), 1500);
		})();
	});
</script>

<Dialog bind:open={u.open} size="xl" chromeless title="Settings" class="settings-dialog">
	<div class="wrap">
		<SettingsNav {searching} />

		<div class="pane" bind:this={paneEl}>
			<div class="close">
				<IconButton variant="text" size="sm" aria-label="Close settings" onclick={() => (u.open = false)}>
					<X size={15} />
				</IconButton>
			</div>
			<div class="page">
				{#if searching}
					<SearchResults
						{results}
						query={u.query}
						onpick={(e: SettingEntry) => u.select({ ...e, item: e.id })}
					/>
				{:else}
					<PageHead title={section.label} description={section.description} />
					{#if tabs}
						<Tabs bind:value={u.tab as SettingsTab}>
							<TabsList>
								{#each tabs as t (t.id)}
									<TabsTrigger value={t.id}>{t.label}</TabsTrigger>
								{/each}
							</TabsList>
						</Tabs>
						<p class="tabdesc">{tab?.description}</p>
					{/if}
					{#key `${u.section}/${tab?.id ?? ""}`}
						<Page />
					{/key}
				{/if}
			</div>
		</div>
	</div>
</Dialog>

<style>
	/* Two panes fill the dialog edge to edge: ~1100 x 780, never larger than the window. */
	:global(.ca-dialog.settings-dialog) {
		/* The size prop sets --dialog-max-w inline (820px for xl), so the property itself is overridden. */
		width: min(1100px, 90vw);
		max-width: min(1100px, 90vw);
		height: min(780px, 88vh);
		max-height: 88vh;
	}
	:global(.settings-dialog .ca-dialog__body) {
		flex: 1 1 auto;
		padding: 0 !important;
		overflow: hidden;
	}
	.wrap {
		display: grid;
		grid-template-columns: 232px minmax(0, 1fr);
		height: 100%;
		min-height: 0;
	}
	.pane {
		position: relative;
		min-height: 0;
		overflow-y: auto;
		padding: 28px 36px 36px;
	}
	.close {
		position: sticky;
		top: 0;
		z-index: 1;
		height: 0;
		display: flex;
		justify-content: flex-end;
		margin: -12px -20px 0 0;
	}
	/* One reading column: rows keep a comfortable measure on a wide dialog. */
	.page {
		max-width: 780px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: 22px;
	}
	/* A scrolling column: children keep their content height (alerts and groups were squeezed). */
	.page > :global(*) {
		flex-shrink: 0;
	}
	.tabdesc {
		margin: -10px 0 0;
		max-width: 72ch;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}
</style>
