import { invoke } from "@tauri-apps/api/core";
import type { HeavyStatus } from "./heavy.ts";

/** The machine-wide heavy-job queue, refreshed by the Activity drawer while it is open. */
class HeavyQueue {
	status = $state<HeavyStatus | null>(null);

	async refresh() {
		try {
			this.status = await invoke<HeavyStatus>("heavy_status");
		} catch {
			// A queue directory that cannot be read is "nothing queued" for a status line.
			this.status = null;
		}
	}
}

export const heavy = new HeavyQueue();
