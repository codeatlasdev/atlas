<script lang="ts">
	/** Settings dialog: section nav on the left, the selected page (or search results) on the right. */
	import { Dialog, IconButton } from "@codeatlasdev/ui";
	import { X } from "@lucide/svelte";
	import { tick } from "svelte";
	import HelpSettings from "#lib/onboarding/HelpSettings.svelte";
	import { searchSettings, type SettingEntry } from "#lib/settings/index.ts";
	import PageHead from "#lib/settings/PageHead.svelte";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import AgentSettings from "./AgentSettings.svelte";
	import GitSettings from "./GitSettings.svelte";
	import AppearanceSettings from "./AppearanceSettings.svelte";
	import EditorSettings from "./EditorSettings.svelte";
	import LinksNotifications from "./settings/LinksNotifications.svelte";
	import SearchResults from "./settings/SearchResults.svelte";
	import SectionTitle from "./settings/SectionTitle.svelte";
	import SettingsNav from "./settings/SettingsNav.svelte";
	import SoundSettings from "./settings/SoundSettings.svelte";
	import UpdatesSection from "./settings/UpdatesSection.svelte";
	import ShellSettings from "./ShellSettings.svelte";
	import ShortcutSettings from "./ShortcutSettings.svelte";
	import SshSettings from "./SshSettings.svelte";
	import TerminalSettings from "./TerminalSettings.svelte";
	import WorkspaceSettings from "./WorkspaceSettings.svelte";

	let results = $derived(searchSettings(u.query));
	let searching = $derived(u.query.trim() !== "");
	let paneEl: HTMLDivElement | undefined = $state();

	/** Jump to the section/tab of a result and flash the matching row once it renders. */
	async function go(e: SettingEntry) {
		u.section = e.section;
		if (e.tab) u.tab = e.tab;
		u.query = "";
		await tick();
		await new Promise((r) => requestAnimationFrame(r));
		const row = paneEl?.querySelector<HTMLElement>(`[data-setting="${e.id}"]`);
		if (!row) return;
		row.scrollIntoView({ block: "center", behavior: "smooth" });
		row.classList.add("flash");
		setTimeout(() => row.classList.remove("flash"), 1500);
	}
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
					<SearchResults {results} query={u.query} onpick={go} />
				{:else if u.section === "general"}
					<PageHead
						title="General"
						description="Keep Atlas up to date, make sure your projects start with the same tools as your terminal, and choose how Atlas tells you things."
					/>
					<UpdatesSection />
					<SectionTitle title="Shell for your projects" setting="shell">
						Services start with the PATH and variables of this shell, the same as in your terminal. Pick
						another shell if tools like bun or node are not found.
					</SectionTitle>
					<ShellSettings />
					<LinksNotifications />
					<SoundSettings />
				{:else if u.section === "appearance"}
					<AppearanceSettings />
				{:else if u.section === "editor"}
					<EditorSettings />
				{:else if u.section === "terminal"}
					<TerminalSettings />
				{:else if u.section === "workspaces"}
					<WorkspaceSettings />
				{:else if u.section === "agents"}
					<AgentSettings />
				{:else if u.section === "git"}
					<GitSettings />
				{:else if u.section === "keyboard"}
					<PageHead
						title="Keyboard shortcuts"
						description="Click a shortcut to change it. Every command is listed, whether it has a key or not."
					/>
					<ShortcutSettings />
				{:else if u.section === "ssh"}
					<SshSettings />
				{:else if u.section === "help"}
					<PageHead
						title="Help & onboarding"
						description="Take the tour again, look up a shortcut, or bring the checklist back."
					/>
					<HelpSettings />
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
</style>
