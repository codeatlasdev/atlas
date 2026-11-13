import { invoke } from "@tauri-apps/api/core";
import type { HygieneReport, MachineReport } from "./machine.ts";

/**
 * State of the Machine drawer, shared by the header button, the command palette and the drawer. The report
 * is refreshed by the drawer only while it is open and the page visible (`usePoll`).
 */
class MachineState {
	open = $state(false);
	report = $state<MachineReport | null>(null);
	/** The report could not be read (the shell is starting, or the command failed): the view says so. */
	failed = $state(false);
	/** `hygiene_report` (#127). Null until that command exists in the app: the section says "not available". */
	hygiene = $state<HygieneReport | null>(null);

	show() {
		this.open = true;
	}

	async refresh() {
		try {
			this.report = await invoke<MachineReport>("machine_report");
			this.failed = false;
		} catch {
			this.failed = true;
		}
		// The typed hook for #127: an app without the command rejects, which is "not available", not an error.
		this.hygiene = await invoke<HygieneReport>("hygiene_report").catch(() => null);
	}
}

export const machine = new MachineState();
