<script lang="ts">
	import { Button, Spinner, toast } from "@codeatlasdev/ui";
	import { homeDir } from "@tauri-apps/api/path";
	import { onMount } from "svelte";
	import { errText } from "#lib/api.ts";
	import { host, hostCall } from "#lib/host.ts";
	import {
		catalog,
		installServer,
		lsp,
		refreshStatus,
		reopenAll,
		restartServer,
		startLspEvents,
	} from "#lib/lsp/client.svelte.ts";
	import { hostGate } from "#lib/lsp/hostGate.ts";
	import { openLog } from "#lib/lsp/log.ts";
	import { invalidateProjectSettings } from "#lib/lsp/projectSettings.ts";
	import type { Catalog } from "#lib/lsp/types.ts";
	import type { LspLanguagePref } from "#lib/prefs.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { prefs, setPref } from "#lib/stores/prefs.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import Group from "./Group.svelte";
	import { openOutsideWorktree } from "./openOutside.ts";
	import ServerRow from "./ServerRow.svelte";
	import { m } from "#lib/i18n/m.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import type { Snippet } from "svelte";

	let data = $state<Catalog | null>(null);
	let loadError = $state("");
	let loading = $state(true);
	/** Servers with an install, uninstall or restart request in flight. */
	let pending = $state<Record<string, boolean>>({});

	let ready = $derived(hostGate.lspReady);
	let lspMap = $derived(prefs.value.lsp);

	async function load() {
		loading = true;
		loadError = "";
		try {
			data = await catalog();
			await refreshStatus();
		} catch (e) {
			loadError = errText(e);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		if (!ready) return;
		startLspEvents();
		void load();
	});

	// A finished install changes what is detected on disk: re-read the catalog.
	let wasBusy = false;
	$effect(() => {
		const busy = lsp.servers.some((s) => s.state === "downloading");
		if (wasBusy && !busy && ready)
			void catalog()
				.then((c) => (data = c))
				.catch(() => {});
		wasBusy = busy;
	});

	async function run(name: string, action: () => Promise<unknown>) {
		pending[name] = true;
		try {
			await action();
			data = await catalog();
		} catch (e) {
			toast.error(errText(e));
		} finally {
			pending[name] = false;
		}
	}

	/** Install, or Update when Atlas already manages a copy (reinstalls the pinned version). */
	const install = (name: string, managed: boolean) => run(name, () => installServer(name, managed));
	const restart = (name: string) => run(name, () => restartServer(name));

	async function uninstall(name: string, label: string) {
		const ok = await confirmDialog(m.settings_editor_languages_tab_uninstall_title({ label }), {
			description: m.settings_editor_languages_tab_uninstall_body(),
			confirmLabel: m.settings_editor_server_row_uninstall(),
			destructive: true,
		});
		if (!ok) return;
		await run(name, async () => {
			await hostCall("lsp_uninstall", { name });
			await refreshStatus();
		});
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			toast.success(m.settings_editor_languages_tab_command_copied());
		} catch (e) {
			toast.error(errText(e));
		}
	}

	/** Persist the `lsp` map, then have open documents re-send their overrides to the host. */
	function save(next: Record<string, LspLanguagePref>) {
		setPref("lsp", next).then(reopenAll, (e) => toast.error(errText(e)));
	}

	/** `<worktree>/.atlas/settings.json`, created as `{ "lsp": {} }` when it does not exist yet. */
	async function openProjectSettings() {
		const w = ws.selected;
		if (!w) {
			toast.info(m.settings_editor_languages_tab_open_worktree_first());
			return;
		}
		const file = `${w.path}/.atlas/settings.json`;
		try {
			const exists = await host.fsStat(file).then(
				() => true,
				() => false,
			);
			if (!exists) {
				await host.fsMkdir(`${w.path}/.atlas`).catch(() => {});
				await host.fsWrite(file, '{\n\t"lsp": {}\n}\n');
			}
			invalidateProjectSettings(w.path);
			openOutsideWorktree(file, ".atlas/settings.json");
		} catch (e) {
			toast.error(errText(e));
		}
	}

	async function openUserSettings() {
		try {
			openOutsideWorktree(`${await homeDir()}/.atlas/desktop/settings.json`, "settings.json");
		} catch (e) {
			toast.error(errText(e));
		}
	}
</script>

{#if !ready}
	<Group title={m.settings_editor_code_tab_language_servers()}>
		<div class="notice" role="status">
			<strong>{m.settings_editor_languages_tab_language_features_need_a_newer()}</strong>
			<span>
				{m.settings_editor_languages_tab_the_running_terminal_host_is()}
			</span>
		</div>
	</Group>
{:else}
	<div data-setting="lsp-servers">
		<Group
			title={m.settings_editor_code_tab_language_servers()}
			description={m.settings_editor_languages_tab_atlas_finds_a_server_on()}
		>
			{#if loading && !data}
				<div class="state">
					<Spinner size={14} />
					{m.settings_editor_languages_tab_loading_language_servers()}
				</div>
			{:else if loadError}
				<div class="state error" role="alert">
					<span>{m.settings_editor_languages_tab_could_not_load_the_language({ loadError })}</span>
					<Button size="sm" variant="outlined" onclick={load}
						>{m.settings_editor_languages_tab_try_again()}</Button
					>
				</div>
			{:else if data}
				{#each data.servers as entry, i (entry.name)}
					<ServerRow
						{entry}
						anchor={i === 0}
						servers={lsp.servers}
						lsp={lspMap}
						pending={pending[entry.name] === true}
						oninstall={() => install(entry.name, entry.detected.source === "managed")}
						onuninstall={() => uninstall(entry.name, entry.label)}
						onrestart={() => restart(entry.name)}
						onlog={() => void openLog(entry.name)}
						oncopy={copy}
						onsave={save}
					/>
				{/each}
			{/if}
		</Group>
	</div>

	{#snippet userLink(content: Snippet)}
		<button class="link" onclick={openUserSettings}>{@render content()}</button>
	{/snippet}
	{#snippet projectLink(content: Snippet)}
		<button class="link" onclick={openProjectSettings}><code>{@render content()}</code></button>
	{/snippet}
	<p class="foot" data-setting="lsp-project">
		<Rich
			parts={m.settings_editor_languages_tab_note.parts()}
			tags={{ user: userLink, project: projectLink }}
		/>
	</p>
{/if}

<style>
	.notice,
	.state {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 12px 14px;
		font-size: 12px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.notice strong {
		font-size: 13px;
		color: var(--ca-text-primary);
	}
	.state {
		flex-direction: row;
		align-items: center;
		gap: 8px;
	}
	.state.error {
		justify-content: space-between;
		color: var(--ca-semantic-error);
	}
	.foot {
		margin: 0;
		font-size: 11.5px;
		line-height: 1.5;
		color: var(--ca-text-hint);
	}
	.foot code {
		font-family: ui-monospace, monospace;
		font-size: 11px;
	}
	.link {
		border: 0;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--ca-text-secondary);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.link:hover {
		color: var(--ca-text-primary);
	}
</style>
