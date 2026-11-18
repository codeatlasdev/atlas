/** Command registry entries about the current project: app/updates, panel tabs, stack and services. */
import { named, type Command } from "#lib/keymap.svelte.ts";
import { api } from "#lib/api.ts";
import { m } from "#lib/i18n/m.ts";
import { activity } from "#lib/panel/activity.svelte.ts";
import { machine } from "#lib/panel/machine.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

/** Reveal (or open) the Panel / Logs / Config tab of the selected worktree. */
function openKind(kind: "services" | "logs" | "config") {
	const w = ws.selected;
	if (!w) return;
	// The names a tab is stored with: identifiers that the tab label maps to the app's language.
	const labels = { services: "Panel", logs: "Logs", config: "Config" };
	ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
}

export function projectCommands(): Command[] {
	// A plain folder has no stack: Panel, Logs, Config, Activity and Start stay out of the palette there.
	const hasProject = () => !!store.current && store.current.kind !== "folder";
	const running = () => !!store.current?.running;
	return named([
		{ id: "zed::OpenSettings", label: () => m.shell_commands_settings(), run: () => updates.show() },
		{
			id: "atlas::CheckForUpdates",
			label: () =>
				updates.pill
					? m.shell_commands_update_to({ version: updates.release?.version ?? "" })
					: m.shell_commands_check_for_updates(),
			run: () => (updates.pill ? updates.showWhatsNew() : void updates.checkNow()),
		},
		{ id: "atlas::WhatsNew", label: () => m.shell_commands_whats_new(), run: () => updates.showWhatsNew() },
		{
			id: "atlas::ShellEnvironment",
			label: () => m.shell_commands_shell_environment(),
			run: () => updates.show("shell"),
		},
		{
			id: "atlas::KeyboardShortcuts",
			label: () => m.shell_commands_keyboard_shortcuts(),
			run: () => updates.show("keyboard"),
		},
		{
			id: "atlas::Services",
			label: () => m.shell_commands_panel(),
			run: () => openKind("services"),
			when: hasProject,
		},
		{ id: "atlas::Logs", label: () => m.panel_tab_logs(), run: () => openKind("logs"), when: hasProject },
		{
			id: "atlas::Config",
			label: () => m.panel_tab_config(),
			run: () => openKind("config"),
			when: hasProject,
		},
		{
			id: "atlas::ShowActivity",
			label: () => m.shell_commands_show_activity(),
			run: () => activity.show(),
			when: hasProject,
		},
		{
			id: "atlas::ShowMachine",
			label: () => m.shell_commands_show_machine(),
			run: () => machine.show(),
			when: hasProject,
		},
		{
			id: "atlas::StartStack",
			label: () => m.shell_commands_start_stack(),
			run: () => store.run(`stack:${store.current!.id}`, () => api.start(store.current!.id)),
			when: () => hasProject() && !running(),
		},
		{
			id: "atlas::StopStack",
			label: () => m.shell_commands_stop_stack(),
			run: () => store.run(`stack:${store.current!.id}`, () => api.stop(store.current!.id)),
			when: running,
		},
		{
			id: "atlas::RestartService",
			label: () => m.shell_commands_restart_service(),
			run: () => {
				const id = store.current!.id;
				const svc = store.focusedService!;
				store.run(`svc:${id}:${svc}`, () => api.service(id, "restart_service", svc));
			},
			when: () => running() && !!store.focusedService,
		},
	]);
}
