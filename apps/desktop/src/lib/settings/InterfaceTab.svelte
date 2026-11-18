<script lang="ts">
	import { SegmentedControl, Select, Switch } from "@codeatlasdev/ui";
	import { formatNumber } from "#lib/i18n/fmt.ts";
	import { kbd } from "#lib/keys.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { ZOOM_LEVELS } from "#lib/zoom.ts";
	import FontRows from "./FontRows.svelte";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import { savePref } from "./savePref.ts";
	import { m } from "#lib/i18n/m.ts";

	let p = $derived(prefs.value);
	/** The ladder; a value off it (hand-edited settings.json) stays selectable as what it is. */
	let zoomOptions = $derived(
		[...new Set([...ZOOM_LEVELS, p.zoom])]
			.sort((a, b) => a - b)
			.map((z) => ({ value: String(z), label: formatNumber(z, { style: "percent" }) })),
	);
</script>

<Group title={m.settings_theme_interface_tab_zoom()}>
	<Row
		id="ui-zoom"
		label={m.settings_theme_interface_tab_zoom()}
		description={m.settings_theme_interface_tab_zoom_description({
			zoomIn: kbd("cmd-="),
			zoomOut: kbd("cmd--"),
			reset: kbd("cmd-0"),
		})}
	>
		<div class="select">
			<Select
				size="sm"
				value={String(p.zoom)}
				options={zoomOptions}
				onchange={(v) => savePref("zoom", Number(v))}
			/>
		</div>
	</Row>
</Group>

<Group title={m.settings_theme_interface_tab_font_title()}>
	<FontRows
		idPrefix="ui"
		familyLabel={m.settings_theme_interface_tab_font_title()}
		familyDescription={m.settings_theme_interface_tab_font_family_description()}
		sizeLabel={m.settings_theme_interface_tab_font_size()}
		sizeDescription={m.settings_theme_interface_tab_font_size_description()}
		family={p.ui_font_family}
		size={p.ui_font_size}
		onfamily={(v) => savePref("ui_font_family", v)}
		onsize={(v) => savePref("ui_font_size", v)}
	>
		{#snippet preview()}
			<div class="sample" style="font-family: {p.ui_font_family}; font-size: {p.ui_font_size}px">
				{m.settings_theme_interface_tab_font_sample()}
			</div>
		{/snippet}
	</FontRows>
</Group>

<Group title={m.settings_theme_interface_tab_file_panel()}>
	<Row
		id="panel-dock"
		label={m.settings_theme_interface_tab_file_panel_side()}
		description={m.settings_theme_interface_tab_file_panel_side_description()}
	>
		<SegmentedControl
			aria-label={m.settings_theme_interface_tab_file_panel_side()}
			size="sm"
			value={p.project_panel.dock}
			items={[
				{ value: "left", label: m.settings_theme_interface_tab_left() },
				{ value: "right", label: m.settings_theme_interface_tab_right() },
			]}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, dock: v as "left" | "right" })}
		/>
	</Row>
	<Row
		id="panel-indent"
		label={m.settings_theme_interface_tab_folder_indent()}
		description={m.settings_theme_interface_tab_folder_indent_description()}
	>
		<NumInput
			label={m.settings_theme_interface_tab_folder_indent()}
			unit="px"
			value={p.project_panel.indent_size}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, indent_size: v })}
		/>
	</Row>
	<Row
		id="panel-file-icons"
		label={m.settings_theme_interface_tab_file_icons()}
		description={m.settings_theme_interface_tab_file_icons_description()}
	>
		<Switch
			aria-label={m.settings_theme_interface_tab_file_icons()}
			checked={p.project_panel.file_icons}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, file_icons: v })}
		/>
	</Row>
	<Row
		id="panel-folder-icons"
		label={m.settings_theme_interface_tab_folder_icons()}
		description={m.settings_theme_interface_tab_folder_icons_description()}
	>
		<Switch
			aria-label={m.settings_theme_interface_tab_folder_icons()}
			checked={p.project_panel.folder_icons}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, folder_icons: v })}
		/>
	</Row>
</Group>

<Group title={m.settings_theme_interface_tab_split_panes()}>
	<Row
		id="pane-border"
		label={m.settings_theme_interface_tab_active_pane_border()}
		description={m.settings_theme_interface_tab_pane_border_description()}
	>
		<NumInput
			label={m.settings_theme_interface_tab_active_pane_border()}
			unit="px"
			value={p.active_pane_modifiers.border_size}
			onchange={(v) => savePref("active_pane_modifiers", { ...p.active_pane_modifiers, border_size: v })}
		/>
	</Row>
	<Row
		id="pane-opacity"
		label={m.settings_theme_interface_tab_dim_inactive_panes()}
		description={m.settings_theme_interface_tab_dim_inactive_description()}
	>
		<NumInput
			label={m.settings_theme_interface_tab_dim_inactive_panes()}
			value={p.active_pane_modifiers.inactive_opacity}
			onchange={(v) => savePref("active_pane_modifiers", { ...p.active_pane_modifiers, inactive_opacity: v })}
		/>
	</Row>
</Group>

<style>
	.select {
		width: 120px;
	}
	.sample {
		padding: 10px 12px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
	}
</style>
