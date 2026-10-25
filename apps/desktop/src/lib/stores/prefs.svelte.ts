/** Live preferences from settings.json, applied as CSS variables; `setPref` persists one key. */
import { api } from "#lib/api.ts";
import { DEFAULT_PREFS, resolvePrefs, type Prefs } from "#lib/prefs.ts";
import { remeasureEditors } from "#lib/symbols.ts";

export const prefs = $state<{ value: Prefs }>({ value: structuredClone(DEFAULT_PREFS) });

export async function loadPrefs() {
	prefs.value = resolvePrefs(await api.settings());
}

/** Patch a top-level settings key (whole nested object for `terminal`, …). */
export async function setPref<K extends keyof Prefs>(key: K, value: Prefs[K]) {
	prefs.value = resolvePrefs(await api.settingsPatch({ [key]: value }));
}

$effect.root(() => {
	// The editor's metrics, so a change of exactly these (and nothing else) makes the open editors re-measure.
	let metrics = "";
	$effect(() => {
		const p = prefs.value;
		const s = document.documentElement.style;
		s.setProperty("--mono", p.buffer_font_family);
		s.setProperty("--atlas-buffer-font-size", `${p.buffer_font_size}px`);
		s.setProperty("--atlas-buffer-line-height", `${p.buffer_line_height}`);
		const next = `${p.buffer_font_family}|${p.buffer_font_size}|${p.buffer_line_height}`;
		if (next !== metrics) {
			// The first run only records: no editor is open yet, and the view measures itself when it mounts.
			if (metrics) remeasureEditors();
			metrics = next;
		}
		s.setProperty("--atlas-ui-font", p.ui_font_family);
		s.setProperty("--atlas-ui-font-size", `${p.ui_font_size}px`);
		s.setProperty("--atlas-pane-active-border", `${p.active_pane_modifiers.border_size}px`);
		s.setProperty("--atlas-pane-inactive-opacity", `${p.active_pane_modifiers.inactive_opacity}`);
		s.setProperty("--atlas-panel-indent", `${p.project_panel.indent_size}px`);
	});
});
