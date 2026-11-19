<script lang="ts">
	/**
	 * Import a theme: pick a file (Ghostty config, .itermcolors, VS Code theme JSON, Alacritty, Base16 yaml,
	 * Atlas/Zed family JSON), drop one on the dialog, paste the text, or open a .zip / .vsix / folder that
	 * holds several themes. Shows what was detected before anything is saved.
	 */
	import { Button, Checkbox, Dialog, Select, TextArea, TextField, toast } from "@codeatlasdev/ui";
	import { TriangleAlert, Upload } from "@lucide/svelte";
	import { homeDir } from "@tauri-apps/api/path";
	import { api, errText } from "#lib/api.ts";
	import { b64decode, host } from "#lib/host.ts";
	import { aroundSlot } from "#lib/i18n/slot.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import type { ImportResult } from "#lib/theme/import/color.ts";
	import { detectFormat, importAny, type ThemeFormat } from "#lib/theme/import/detect.ts";
	import ImportPreview from "./ImportPreview.svelte";
	import PackageImport from "./PackageImport.svelte";
	import { FORMAT_MESSAGES, appearanceLabel } from "./labels.ts";
	import { m } from "#lib/i18n/m.ts";

	let { open = $bindable(false) }: { open: boolean } = $props();

	/** Two lines of a Ghostty config as the text area's placeholder: syntax, not words, so it is never translated. */
	const SAMPLE_CONFIG = "palette = 0=#21222c\nbackground = #282a36";

	let fileName = $state("");
	let text = $state("");
	let format = $state<ThemeFormat | null>(null);
	let result = $state<ImportResult | null>(null);
	let error = $state<string | null>(null);
	let name = $state("");
	let appearance = $state<"dark" | "light">("dark");
	let applyFont = $state(false);
	let select = $state(true);
	let busy = $state(false);
	let dragging = $state(false);
	/** A package (zip / vsix / folder) is loaded: its list replaces the single-file form. */
	let packaged = $state(false);

	/** Zed/Atlas families are copied verbatim; every other format is converted to an `ImportResult`. */
	let copyOnly = $derived(format === "zed" || format === "atlas");
	let hasPrefs = $derived(!!result && (!!result.font || !!result.terminal));
	let ready = $derived(copyOnly || !!result);
	let preview = $derived<ImportResult | null>(result && { ...result, name, appearance });
	/** The text around the detected format's name (shown in bold); a format copied as is says so in the same sentence. */
	let detected = $derived(
		aroundSlot((slot) =>
			copyOnly
				? m.settings_theme_import_dialog_copy_only({ format: slot })
				: m.settings_theme_import_dialog_detected({ format: slot }),
		),
	);

	// A closed dialog starts empty again.
	$effect(() => {
		if (open) return;
		fileName = text = name = "";
		format = null;
		result = error = null;
		applyFont = false;
		select = true;
	});

	async function analyze() {
		result = error = null;
		format = text.trim() ? detectFormat(fileName, text) : null;
		if (!text.trim()) return;
		try {
			const home = await homeDir();
			const outcome = await importAny(fileName || "pasted", text, {
				home,
				readFile: async (p) => {
					try {
						return b64decode((await host.fsRead(p, 2_000_000)).data);
					} catch {
						return null;
					}
				},
				themeDirs: [
					`${home}/.config/ghostty/themes`,
					"/Applications/Ghostty.app/Contents/Resources/ghostty/themes",
				],
				appearance: themes.current?.appearance ?? "dark",
			});
			format = outcome.kind;
			if ("result" in outcome) {
				result = outcome.result;
				name = outcome.result.name;
				appearance = outcome.result.appearance;
				applyFont = !!(outcome.result.font || outcome.result.terminal);
			}
		} catch (e) {
			error = errText(e);
		}
	}

	async function load(path: string, content: string) {
		fileName = path;
		text = content;
		await analyze();
	}

	async function choose() {
		const path = await api.pickPath({
			title: m.settings_theme_import_dialog_import_a_theme(),
			filters: [
				{
					name: m.settings_theme_import_dialog_filter_theme(),
					extensions: ["json", "jsonc", "itermcolors", "yaml", "yml", "conf", "txt"],
				},
				{ name: m.settings_theme_import_dialog_filter_any(), extensions: ["*"] },
			],
		});
		if (typeof path !== "string") return;
		try {
			await load(path, b64decode((await host.fsRead(path, 2_000_000)).data));
		} catch (e) {
			error = errText(e);
		}
	}

	async function drop(e: DragEvent) {
		e.preventDefault();
		dragging = false;
		const file = e.dataTransfer?.files[0];
		if (!file || packaged) return;
		if (/\.(zip|vsix)$/i.test(file.name)) {
			error = m.settings_theme_import_dialog_archive_dropped();
			return;
		}
		await load(file.name, await file.text());
	}

	/** Copy a Zed/Atlas family file as is into `~/.atlas/themes`. */
	async function copyFamily() {
		const dir = `${await homeDir()}/.atlas/themes`;
		const base = (fileName.split("/").pop() || "theme.json").replace(/\.[^.]+$/, "");
		await host.fsMkdir(dir).catch(() => {});
		await host.fsWrite(`${dir}/${base || "theme"}.json`, text);
		await themes.reload();
		toast.success(m.settings_theme_import_dialog_file_added());
	}

	async function run() {
		busy = true;
		try {
			if (copyOnly) await copyFamily();
			else if (preview) {
				const t = await themes.importResult(preview, { applyFont: applyFont && hasPrefs, select });
				toast.success(
					select
						? m.settings_theme_toast_using({ name: t.name })
						: m.settings_theme_import_dialog_imported({ name: t.name }),
				);
			}
			open = false;
		} catch (e) {
			error = errText(e);
		} finally {
			busy = false;
		}
	}
</script>

<Dialog
	bind:open
	title={m.settings_theme_import_dialog_import_a_theme()}
	description={m.settings_theme_import_dialog_description()}
	size="lg"
>
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="body"
		class:dragging
		ondragover={(e) => {
			e.preventDefault();
			dragging = true;
		}}
		ondragleave={() => (dragging = false)}
		ondrop={drop}
	>
		<PackageImport bind:open bind:active={packaged} onerror={(message) => (error = message)} />
		{#if !packaged}
			<div class="pick">
				<Button variant="outlined" onclick={choose}
					><Upload size={13} /> {m.settings_theme_import_dialog_choose_file()}</Button
				>
				<span class="hint">{m.settings_theme_import_dialog_drop_hint()}</span>
			</div>
			<TextArea
				label={m.settings_theme_import_dialog_theme_text()}
				rows={5}
				placeholder={SAMPLE_CONFIG}
				bind:value={text}
				oninput={() => {
					fileName = "";
				}}
				onchange={analyze}
			/>
		{/if}

		{#if error}
			<p class="error" role="alert"><TriangleAlert size={13} /> {error}</p>
		{:else if format}
			<p class="detected" role="status">
				{detected[0]}<b>{FORMAT_MESSAGES[format]()}</b>{detected[1]}
			</p>
		{/if}

		{#if !packaged && preview && result}
			<div class="grid">
				<div class="fields">
					<TextField label={m.settings_theme_import_dialog_name()} bind:value={name} />
					<Select
						label={m.settings_theme_import_dialog_appearance()}
						value={appearance}
						options={[
							{ value: "dark", label: appearanceLabel("dark") },
							{ value: "light", label: appearanceLabel("light") },
						]}
						onchange={(v) => (appearance = v as "dark" | "light")}
					/>
					{#if hasPrefs}
						<Checkbox bind:checked={applyFont} label={m.settings_theme_import_dialog_apply_font()} />
					{/if}
					<Checkbox
						bind:checked={select}
						label={appearance === "dark"
							? m.settings_theme_import_dialog_use_as_dark()
							: m.settings_theme_import_dialog_use_as_light()}
					/>
				</div>
				<ImportPreview result={preview} {name} />
			</div>
			{#if result.warnings.length}
				<ul class="warnings" aria-label={m.settings_theme_import_dialog_import_warnings()}>
					{#each result.warnings as w, i (i)}<li>{w}</li>{/each}
				</ul>
			{/if}
		{/if}
	</div>

	{#snippet actions()}
		<Button variant="ghost" onclick={() => (open = false)}>{m.common_action_cancel()}</Button>
		{#if !packaged}
			<Button disabled={!ready || busy || (!copyOnly && !name.trim())} loading={busy} onclick={run}
				>{m.settings_theme_import_dialog_import()}</Button
			>
		{/if}
	{/snippet}
</Dialog>

<style>
	.body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		border-radius: 10px;
	}
	.body.dragging {
		outline: 2px dashed var(--atlas-focus-ring, var(--ca-brand-main));
		outline-offset: 4px;
	}
	.pick {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.detected,
	.error {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.error {
		color: var(--ca-semantic-error);
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 240px;
		gap: 16px;
		align-items: start;
	}
	.fields {
		display: flex;
		flex-direction: column;
		gap: 10px;
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
</style>
