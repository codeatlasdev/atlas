/** Command registry entries about the current project: app/updates, panel tabs, stack and services. */
import type { Command } from "#lib/keymap.svelte.ts";
import { api } from "#lib/api.ts";
import { activity } from "#lib/panel/activity.svelte.ts";
import { machine } from "#lib/panel/machine.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { updates } from "#lib/stores/updates.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

/** Reveal (or open) the Panel / Logs / Config tab of the selected worktree. */
function openKind(kind: "services" | "logs" | "config") {
	const w = ws.selected;
	if (!w) return;
	const labels = { services: "Panel", logs: "Logs", config: "Config" };
	ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
}

export function projectCommands(): Command[] {
	const hasProject = () => !!store.current;
	const running = () => !!store.current?.running;
	return [
		{ id: "zed::OpenSettings", label: "Settings", run: () => updates.show() },
		{
			id: "atlas::CheckForUpdates",
			get label() {
				return updates.pill ? `Update to ${updates.release?.version}` : "Check for updates";
			},
			run: () => (updates.pill ? updates.showWhatsNew() : void updates.checkNow()),
		},
		{ id: "atlas::WhatsNew", label: "What's new", run: () => updates.showWhatsNew() },
		{ id: "atlas::ShellEnvironment", label: "Shell environment", run: () => updates.show("shell") },
		{ id: "atlas::KeyboardShortcuts", label: "Keyboard shortcuts", run: () => updates.show("keyboard") },
		{ id: "atlas::Services", label: "Panel", run: () => openKind("services"), when: hasProject },
		{ id: "atlas::Logs", label: "Logs", run: () => openKind("logs"), when: hasProject },
		{ id: "atlas::Config", label: "Config", run: () => openKind("config"), when: hasProject },
		{ id: "atlas::ShowActivity", label: "Show activity", run: () => activity.show(), when: hasProject },
		{
			id: "atlas::ShowMachine",
			label: "Show machine load and agents",
			run: () => machine.show(),
			when: hasProject,
		},
		{
			id: "atlas::StartStack",
			label: "Start stack",
			run: () => store.run(`stack:${store.current!.id}`, () => api.start(store.current!.id)),
			when: () => hasProject() && !running(),
		},
		{
			id: "atlas::StopStack",
			label: "Stop stack",
			run: () => store.run(`stack:${store.current!.id}`, () => api.stop(store.current!.id)),
			when: running,
		},
		{
			id: "atlas::RestartService",
			label: "Restart focused service",
			run: () => {
				const id = store.current!.id;
				const svc = store.focusedService!;
				store.run(`svc:${id}:${svc}`, () => api.service(id, "restart_service", svc));
			},
			when: () => running() && !!store.focusedService,
		},
	];
}
