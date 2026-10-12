/**
 * UI state of the Action output drawer, shared by the header, the palette command, the
 * Panel's operation strip and the drawer itself.
 */
class ActionOutputState {
	open = $state(false);
	/** Run shown, per project; `null` follows the newest run. */
	selected = $state<Record<string, number | null>>({});

	/** Open the drawer, on `runId` or (default) the newest run. */
	show(project: string, runId: number | null = null) {
		this.selected[project] = runId;
		this.open = true;
	}
}

export const actionOutput = new ActionOutputState();
