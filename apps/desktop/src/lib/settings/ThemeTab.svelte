<script lang="ts">
	import { Button, Select, SegmentedControl, THEME_PRESETS, colorScheme } from "@codeatlasdev/ui";
	import { Check, FolderOpen, Monitor, Moon, Sun, Upload } from "@lucide/svelte";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { readableOn } from "#lib/theme/color.ts";
	import Group from "./Group.svelte";
	import Row from "./Row.svelte";
	import { copyTheme, revealThemesFolder, saveTheme } from "./theme/actions.ts";
	import CreditCard from "./theme/CreditCard.svelte";
	import GhosttyBanner from "./theme/GhosttyBanner.svelte";
	import ImportDialog from "./theme/ImportDialog.svelte";
	import TerminalThemeSelect from "./theme/TerminalThemeSelect.svelte";
	import ThemeGrid from "./theme/ThemeGrid.svelte";
	import ThemeIssues from "./theme/ThemeIssues.svelte";
	import { accentLabel } from "./theme/labels.ts";
	import { m } from "#lib/i18n/m.ts";

	let modes = $derived([
		{ value: "system", label: m.settings_theme_mode_system(), icon: Monitor },
		{ value: "light", label: m.settings_theme_mode_light(), icon: Sun },
		{ value: "dark", label: m.settings_theme_mode_dark(), icon: Moon },
	]);

	let importing = $state(false);
	/** Theme the Export row acts on; follows the active theme until the user picks another. */
	let exportName = $state<string | null>(null);
	let exportTarget = $derived(exportName ?? themes.current?.name ?? "");
	const options = (names: string[]) => names.map((n) => ({ value: n, label: n }));
</script>

<GhosttyBanner />

<Group title={m.settings_registry_appearance_group_color_theme()}>
	<Row
		id="theme-mode"
		label={m.settings_registry_appearance_theme_mode_label()}
		description={m.settings_theme_tab_theme_description()}
	>
		<SegmentedControl
			aria-label={m.settings_registry_appearance_theme_mode_label()}
			size="sm"
			value={themes.selection.mode}
			items={modes}
			onchange={(v) => void themes.setTheme({ mode: v as "system" | "light" | "dark" })}
		/>
	</Row>
	<CreditCard />
	<ThemeGrid />
	<Row
		id="theme-light"
		label={m.settings_registry_appearance_theme_light_label()}
		description={m.settings_theme_tab_light_theme_description()}
	>
		<div class="select">
			<Select
				size="sm"
				value={themes.selection.light}
				options={options(themes.names.light)}
				onchange={(v) => void themes.setTheme({ light: v })}
			/>
		</div>
	</Row>
	<Row
		id="theme-dark"
		label={m.settings_registry_appearance_theme_dark_label()}
		description={m.settings_theme_tab_dark_theme_description()}
	>
		<div class="select">
			<Select
				size="sm"
				value={themes.selection.dark}
				options={options(themes.names.dark)}
				onchange={(v) => void themes.setTheme({ dark: v })}
			/>
		</div>
	</Row>
	<Row
		id="theme-terminal"
		label={m.settings_registry_appearance_theme_terminal_label()}
		description={m.settings_theme_tab_terminal_theme_description()}
	>
		<TerminalThemeSelect />
	</Row>
	<ThemeIssues />
</Group>

<Group title={m.settings_theme_tab_import_and_export()}>
	<Row
		id="theme-import"
		stack
		label={m.settings_registry_appearance_theme_import_label()}
		description={m.settings_theme_tab_import_description()}
	>
		<Button size="sm" variant="outlined" onclick={() => (importing = true)}>
			<Upload size={13} />
			{m.settings_theme_tab_import()}
		</Button>
		<Button size="sm" variant="ghost" onclick={revealThemesFolder}>
			<FolderOpen size={13} />
			{m.settings_theme_tab_reveal_themes_folder()}
		</Button>
	</Row>
	<Row
		id="theme-export"
		stack
		label={m.settings_registry_appearance_theme_export_label()}
		description={m.settings_theme_tab_export_description()}
	>
		<div class="select">
			<Select
				size="sm"
				value={exportTarget}
				options={options(themes.list.map((t) => t.name))}
				onchange={(v) => (exportName = v)}
			/>
		</div>
		<Button
			size="sm"
			variant="outlined"
			disabled={!exportTarget}
			onclick={() => copyTheme(exportTarget, "json")}
		>
			{m.settings_theme_tab_copy_as_json()}
		</Button>
		<Button
			size="sm"
			variant="outlined"
			disabled={!exportTarget}
			onclick={() => copyTheme(exportTarget, "ghostty")}
		>
			{m.settings_theme_tab_copy_as_ghostty_config()}
		</Button>
		<Button
			size="sm"
			variant="ghost"
			disabled={!exportTarget}
			onclick={() => saveTheme(exportTarget, "json")}
		>
			{m.settings_theme_tab_save_to_file()}
		</Button>
	</Row>
</Group>

<Group title={m.settings_registry_appearance_accent_label()}>
	<Row
		id="accent"
		label={m.settings_registry_appearance_accent_label()}
		description={m.settings_registry_appearance_accent_description()}
	>
		<div class="swatches">
			{#each THEME_PRESETS as t (t.id)}
				<button
					class="sw"
					class:on={colorScheme.preset === t.id}
					style="--sw: {t.swatchColor}; --on: {readableOn(t.swatchColor)}"
					title={accentLabel(t)}
					aria-label={accentLabel(t)}
					aria-pressed={colorScheme.preset === t.id}
					onclick={() => colorScheme.setPreset(t.id)}
				>
					{#if colorScheme.preset === t.id}<Check size={13} />{/if}
				</button>
			{/each}
		</div>
	</Row>
</Group>

<ImportDialog bind:open={importing} />

<style>
	.select {
		width: 200px;
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 10px;
		max-width: 260px;
	}
	.sw {
		width: 26px;
		height: 26px;
		border-radius: 50%;
		border: 0;
		background: var(--sw);
		color: var(--on);
		display: grid;
		place-items: center;
		box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.2);
	}
	.sw.on {
		box-shadow:
			0 0 0 2px var(--ca-surface-raised),
			0 0 0 4px var(--sw);
	}
</style>
