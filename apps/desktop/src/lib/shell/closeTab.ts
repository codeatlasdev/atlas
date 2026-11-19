/** ⌘W: close the focused tab, asking first when a terminal still runs a foreground program. */
import { toast } from "@codeatlasdev/ui";
import { host } from "#lib/host.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { m } from "#lib/i18n/m.ts";
import { pinned } from "#lib/stores/tabs.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

export async function closeActive() {
	const w = ws.selected;
	if (!w) return;
	const g = ws.focusedGroup(w.id);
	const t = ws.tab(g.activeTab);
	if (!t) return;
	if (pinned(t)) {
		// The Panel stays open: tell the user once, and move on to a neighbour.
		toast.info(m.shell_commands_panel_stays_open());
		const next = g.order.find((id) => !pinned(ws.tab(id)));
		if (next) ws.activate(next);
		return;
	}
	if (t.pty && !(t.pty in ws.exited)) {
		const info = await host.ptyInfo(t.pty).catch(() => null);
		const shell = (t.argv?.length ? t.argv[0] : info?.argv[0])?.split("/").pop();
		if (
			info?.fg &&
			info.fg.name.replace(/^-/, "") !== shell &&
			!(await confirmDialog(m.shell_commands_still_running({ name: info.fg.name }), {
				description: m.shell_commands_closing_ends_it(),
				confirmLabel: m.shell_commands_close_tab(),
				destructive: true,
			}))
		)
			return;
	}
	ws.closeTab(t.id);
}
