<script lang="ts">
	import { SegmentedControl, Select, Switch } from "@codeatlasdev/ui";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { ZOOM_LEVELS } from "#lib/zoom.ts";
	import FontRows from "./FontRows.svelte";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import { savePref } from "./savePref.ts";

	let p = $derived(prefs.value);
	/** The ladder; a value off it (hand-edited settings.json) stays selectable as what it is. */
	let zoomOptions = $derived(
		[...new Set([...ZOOM_LEVELS, p.zoom])]
			.sort((a, b) => a - b)
			.map((z) => ({ value: String(z), label: `${Math.round(z * 100)}%` })),
	);
</script>

<Group title="Zoom">
	<Row
		id="ui-zoom"
		label="Zoom"
		description="Scale the whole window. ⌘+ and ⌘− step it, ⌘0 resets it (Ctrl instead of ⌘ outside macOS)."
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

<Group title="Interface font">
	<FontRows
		idPrefix="ui"
		familyLabel="Interface font"
		familyDescription="Font for menus, sidebars and buttons."
		sizeLabel="Interface font size"
		sizeDescription="Text size across the app."
		family={p.ui_font_family}
		size={p.ui_font_size}
		onfamily={(v) => savePref("ui_font_family", v)}
		onsize={(v) => savePref("ui_font_size", v)}
	>
		{#snippet preview()}
			<div class="sample" style="font-family: {p.ui_font_family}; font-size: {p.ui_font_size}px">
				Services, worktrees and agents, all in one place.
			</div>
		{/snippet}
	</FontRows>
</Group>

<Group title="File panel">
	<Row id="panel-dock" label="File panel side" description="Show the file tree on the left or the right.">
		<SegmentedControl
			aria-label="File panel side"
			size="sm"
			value={p.project_panel.dock}
			items={[
				{ value: "left", label: "Left" },
				{ value: "right", label: "Right" },
			]}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, dock: v as "left" | "right" })}
		/>
	</Row>
	<Row id="panel-indent" label="Folder indent" description="How far each folder level is pushed in.">
		<NumInput
			label="Folder indent"
			unit="px"
			value={p.project_panel.indent_size}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, indent_size: v })}
		/>
	</Row>
	<Row id="panel-file-icons" label="File icons" description="Show an icon next to each file.">
		<Switch
			aria-label="File icons"
			checked={p.project_panel.file_icons}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, file_icons: v })}
		/>
	</Row>
	<Row id="panel-folder-icons" label="Folder icons" description="Show an icon next to each folder.">
		<Switch
			aria-label="Folder icons"
			checked={p.project_panel.folder_icons}
			onchange={(v) => savePref("project_panel", { ...p.project_panel, folder_icons: v })}
		/>
	</Row>
</Group>

<Group title="Split panes">
	<Row
		id="pane-border"
		label="Active pane border"
		description="Outline the pane you are working in. 0 hides it."
	>
		<NumInput
			label="Active pane border"
			unit="px"
			value={p.active_pane_modifiers.border_size}
			onchange={(v) => savePref("active_pane_modifiers", { ...p.active_pane_modifiers, border_size: v })}
		/>
	</Row>
	<Row
		id="pane-opacity"
		label="Dim inactive panes"
		description="How visible panes you are not using stay: 0.2 is faint, 1 is not dimmed."
	>
		<NumInput
			label="Dim inactive panes"
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
