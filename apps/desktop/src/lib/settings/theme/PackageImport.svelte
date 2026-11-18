<script lang="ts">
	/**
	 * Import every theme found in a `.zip`, `.vsix` or folder. The archive is unpacked to a temporary
	 * folder on this computer by the Rust side, scanned there, and the folder is always removed afterwards.
	 */
	import { Button, Checkbox, toast } from "@codeatlasdev/ui";
	import { Archive, ChevronDown, ChevronRight, FolderOpen, TriangleAlert } from "@lucide/svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { open as pickPath } from "@tauri-apps/plugin-dialog";
	import { errText } from "#lib/api.ts";
	import { aroundSlot } from "#lib/i18n/slot.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { scanPackage, type FoundFormat, type FoundTheme } from "#lib/theme/import/package.ts";
	import ImportPreview from "./ImportPreview.svelte";
	import { appearanceLabel } from "./labels.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Unpacked {
		dir: string;
		temporary: boolean;
		files: { rel: string; size: number }[];
	}

	let {
		open = $bindable(false),
		active = $bindable(false),
		onerror,
	}: { open: boolean; active: boolean; onerror: (message: string | null) => void } = $props();

	/** Product names of the apps a theme comes from: never translated. */
	const FORMAT_LABEL: Record<FoundFormat, string> = {
		vscode: "VS Code",
		ghostty: "Ghostty",
		iterm: "iTerm",
		alacritty: "Alacritty",
		base16: "Base16",
		zed: "Zed",
	};
	const THEMES_FOLDER = "~/.atlas/themes";

	let found = $state<FoundTheme[]>([]);
	let skipped = $state<string[]>([]);
	let warnings = $state<string[]>([]);
	let picked = $state<Record<string, boolean>>({});
	let focusId = $state<string | null>(null);
	let scanning = $state(false);
	let importing = $state(false);
	let showNotes = $state(false);
	/** Names that failed during the last import, shown until the next attempt. */
	let failures = $state<string[]>([]);
	/** Set when exactly one theme was imported: the dialog offers `Use it`. */
	let importedOne = $state<string | null>(null);
	/** Temporary folder to delete; null when nothing is unpacked or the folder is the user's own. */
	let tempDir: string | null = null;

	let chosen = $derived(found.filter((f) => picked[f.id]));
	let focused = $derived(found.find((f) => f.id === focusId) ?? found[0]);
	let notes = $derived(warnings.length + skipped.length);
	// The folder path (code) and the imported theme's name (bold) sit inside their sentences: the message places them.
	let hint = $derived(aroundSlot((path) => m.settings_theme_import_package_hint({ path })));
	let imported = $derived(aroundSlot((name) => m.settings_theme_import_package_imported_one({ name })));

	async function cleanup() {
		const dir = tempDir;
		tempDir = null;
		if (dir) await invoke("theme_unpack_cleanup", { dir }).catch(() => {});
	}

	function reset() {
		found = [];
		skipped = [];
		warnings = [];
		picked = {};
		focusId = null;
		failures = [];
		importedOne = null;
		showNotes = false;
		active = false;
	}

	// A closed dialog drops the package and removes its temporary folder.
	$effect(() => {
		if (open) return;
		reset();
		void cleanup();
	});

	async function choose(kind: "archive" | "folder") {
		const path = await pickPath(
			kind === "folder"
				? { directory: true, multiple: false, title: m.settings_theme_import_package_choose_folder_title() }
				: {
						multiple: false,
						title: m.settings_theme_import_package_choose_archive_title(),
						filters: [
							{ name: m.settings_theme_import_package_filter_archive(), extensions: ["zip", "vsix"] },
						],
					},
		);
		if (typeof path !== "string") return;
		scanning = true;
		onerror(null);
		await cleanup();
		reset();
		try {
			const pkg = await invoke<Unpacked>("theme_unpack", { path });
			if (pkg.temporary) tempDir = pkg.dir;
			const scan = await scanPackage(pkg.files, {
				read: (rel) => invoke<string>("theme_read_packed", { dir: pkg.dir, rel }).catch(() => null),
			});
			found = scan.themes;
			skipped = scan.skipped;
			warnings = scan.warnings;
			picked = Object.fromEntries(scan.themes.map((t) => [t.id, true]));
			focusId = scan.themes[0]?.id ?? null;
			active = true;
			if (!scan.themes.length) onerror(m.settings_theme_import_package_none_found());
		} catch (e) {
			onerror(errText(e));
			await cleanup();
		} finally {
			scanning = false;
		}
	}

	const selectAll = (on: boolean) => (picked = Object.fromEntries(found.map((t) => [t.id, on])));

	async function importChosen() {
		importing = true;
		failures = [];
		onerror(null);
		const done: string[] = [];
		try {
			for (const t of chosen) {
				try {
					const saved = await themes.importResult(t.result, { select: false });
					done.push(saved.name);
				} catch (e) {
					failures.push(`${t.name}: ${errText(e)}`);
				}
			}
			if (done.length) {
				toast.success(m.settings_theme_import_package_toast_imported({ count: done.length }));
			}
			if (done.length === 1 && !failures.length) importedOne = done[0]!;
			else if (!failures.length) open = false;
			else onerror(m.settings_theme_import_package_failed_count({ count: failures.length }));
		} finally {
			importing = false;
			await cleanup();
		}
	}

	function useIt() {
		const name = importedOne;
		const t = themes.list.find((x) => x.name === name);
		if (t) void themes.setTheme({ [t.appearance]: t.name });
		open = false;
	}
</script>

<div class="pick">
	<Button variant="outlined" loading={scanning} disabled={importing} onclick={() => choose("archive")}>
		<Archive size={13} />
		{m.settings_theme_import_package_archive()}
	</Button>
	<Button variant="outlined" disabled={scanning || importing} onclick={() => choose("folder")}>
		<FolderOpen size={13} />
		{m.settings_theme_import_package_folder()}
	</Button>
</div>
<p class="hint">
	{hint[0]}<code>{THEMES_FOLDER}</code>{hint[1]}
</p>

{#if importedOne}
	<div class="done" role="status">
		<span>{imported[0]}<b>{importedOne}</b>{imported[1]}</span>
		<Button size="sm" variant="filled" onclick={useIt}>{m.settings_theme_use_it()}</Button>
		<Button size="sm" variant="ghost" onclick={() => (open = false)}>{m.common_action_done()}</Button>
	</div>
{:else if active && found.length}
	<div class="bar">
		<span class="count" role="status"
			>{m.settings_theme_import_package_selected({ selected: chosen.length, total: found.length })}</span
		>
		<Button size="sm" variant="ghost" onclick={() => selectAll(true)}
			>{m.settings_theme_import_package_select_all()}</Button
		>
		<Button size="sm" variant="ghost" onclick={() => selectAll(false)}
			>{m.settings_theme_import_package_select_none()}</Button
		>
	</div>
	<div class="list">
		<ul class="rows" aria-label={m.settings_theme_import_package_themes_found()}>
			{#each found as t (t.id)}
				<li class="row" class:focus={focused?.id === t.id}>
					<Checkbox
						aria-label={m.settings_theme_import_package_import_name({ name: t.name })}
						checked={picked[t.id] ?? false}
						onchange={(c) => (picked[t.id] = c)}
					/>
					<button
						type="button"
						class="item"
						aria-label={m.settings_theme_import_package_preview_name({ name: t.name })}
						aria-current={focused?.id === t.id}
						onclick={() => (focusId = t.id)}
						onfocus={() => (focusId = t.id)}
					>
						<span class="name">{t.name}</span>
						<span class="appearance">{appearanceLabel(t.appearance)}</span>
						<span class="chips">
							<span class="chip">{FORMAT_LABEL[t.format]}</span>
							{#if t.sources.length > 1}<span class="chip"
									>{m.settings_theme_import_package_files({ count: t.sources.length })}</span
								>{/if}
						</span>
						<span class="strip" aria-hidden="true">
							{#each t.swatches as c, i (i)}<span style:background={c}></span>{/each}
						</span>
					</button>
				</li>
			{/each}
		</ul>
		{#if focused}<div class="side"><ImportPreview result={focused.result} name={focused.name} /></div>{/if}
	</div>
	{#if notes}
		<div class="notes">
			<button type="button" class="toggle" aria-expanded={showNotes} onclick={() => (showNotes = !showNotes)}>
				{#if showNotes}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
				<TriangleAlert size={13} aria-hidden="true" />
				{m.settings_theme_import_package_notes_summary({
					warnings: warnings.length,
					skipped: skipped.length,
				})}
			</button>
			{#if showNotes}
				<ul class="warnings" aria-label={m.settings_theme_import_package_notes_label()}>
					{#each warnings as w, i (`w${i}`)}<li>{w}</li>{/each}
					{#each skipped as s, i (`s${i}`)}<li>
							{m.settings_theme_import_package_skipped({ file: s })}
						</li>{/each}
				</ul>
			{/if}
		</div>
	{/if}
	{#if failures.length}
		<ul class="warnings" role="alert" aria-label={m.settings_theme_import_package_failures_label()}>
			{#each failures as f, i (i)}<li>{f}</li>{/each}
		</ul>
	{/if}
	<div class="go">
		<Button
			variant="filled"
			disabled={!chosen.length || importing}
			loading={importing}
			onclick={importChosen}
		>
			{m.settings_theme_import_package_import_n({ count: chosen.length })}
		</Button>
	</div>
{/if}

<style>
	.pick {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.hint {
		margin: 0;
		font-size: 12px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.count {
		flex: 1;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.list {
		display: grid;
		grid-template-columns: 1fr 220px;
		gap: 16px;
		align-items: start;
	}
	.rows {
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 280px;
		margin: 0;
		padding: 0;
		overflow-y: auto;
		list-style: none;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 2px 6px;
		border-radius: 8px;
	}
	.row.focus {
		background: var(--ca-action-hover);
	}
	.item {
		display: grid;
		grid-template-columns: 1fr auto;
		grid-template-areas:
			"name chips"
			"appearance strip";
		flex: 1;
		min-width: 0;
		align-items: center;
		gap: 2px 8px;
		padding: 4px 6px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		text-align: left;
		color: var(--ca-text-primary);
		cursor: pointer;
	}
	.item:focus-visible {
		outline: 2px solid var(--atlas-focus-ring, var(--ca-brand-main));
		outline-offset: 1px;
	}
	.name {
		grid-area: name;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 12.5px;
		font-weight: 550;
	}
	.appearance {
		grid-area: appearance;
		font-size: 11px;
		text-transform: capitalize;
		color: var(--ca-text-secondary);
	}
	.chips {
		grid-area: chips;
		display: flex;
		gap: 4px;
	}
	.chip {
		padding: 0 5px;
		border-radius: 999px;
		font-size: 10px;
		line-height: 15px;
		color: var(--ca-text-secondary);
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.strip {
		grid-area: strip;
		display: flex;
		justify-self: end;
		overflow: hidden;
		border-radius: 4px;
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.strip span {
		width: 12px;
		height: 10px;
	}
	.notes {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		align-self: flex-start;
		padding: 2px 4px;
		border: 0;
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.toggle:focus-visible {
		outline: 2px solid var(--atlas-focus-ring, var(--ca-brand-main));
	}
	.warnings {
		margin: 0;
		padding: 8px 10px 8px 26px;
		border-radius: 8px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, transparent);
	}
	.go,
	.done {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
	}
	.done {
		justify-content: flex-start;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
</style>
