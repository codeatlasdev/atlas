import { m } from "#lib/i18n/m.ts";
import type { Tab } from "#lib/stores/workspaceTypes.ts";

/**
 * The built-in names a tab is created with. They are stored in the workspace document as the English the tab
 * opened with, so the screen maps them to the app's language when it draws the tab: a workspace saved in English
 * reads in Portuguese after the language changes, and the stored text stays a plain identifier for `ctl`.
 * A name the person chose (Rename sets `customLabel`) or a name that is not here (a file, a host, `gh auth`)
 * shows as stored.
 */
const BUILT_IN: Record<string, () => string> = {
	Terminal: () => m.crew_chat_header_terminal(),
	Setup: () => m.workbench_tab_setup(),
	Panel: () => m.workbench_tab_panel(),
	Logs: () => m.panel_tab_logs(),
	Config: () => m.panel_tab_config(),
	Changes: () => m.workbench_tab_changes(),
	"Pull request": () => m.workbench_tab_pull_request(),
};

/**
 * What a tab shows as its name: the person's own name, else (for a terminal) the title its program set, else the
 * built-in name in the app's language. The project Panel tab is always called Panel, as tabs saved before it had
 * that name keep their old stored label.
 */
export function tabLabel(t: Pick<Tab, "label" | "customLabel" | "kind">, title?: string): string {
	if (t.customLabel) return t.label;
	if (t.kind === "services") return BUILT_IN.Panel();
	if (title) return title;
	return Object.hasOwn(BUILT_IN, t.label) ? BUILT_IN[t.label]() : t.label;
}
