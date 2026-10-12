import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "@codeatlasdev/ui";

/** Native GPU terminal for local terminals; on unless the user turned it off (per machine). */
const KEY = "atlas.gpuTerminal";

export const gpu = $state({
	terminal: localStorage.getItem(KEY) !== "0",
	/** The running terminal host predates the raw byte stream and cannot be replaced without
	 * ending terminals (it outlives app updates): terminals use xterm.js until it restarts. */
	hostTooOld: false,
});

export function setGpuTerminal(on: boolean) {
	gpu.terminal = on;
	localStorage.setItem(KEY, on ? "1" : "0");
}

interface HostStatus {
	status: "current" | "upgraded" | "outdated";
	detail: unknown;
}

/** An `upgraded` status is toasted once per app session, however many times it is re-read. */
let toasted = false;

export function applyHostStatus(s: HostStatus | null) {
	if (!s) return;
	gpu.hostTooOld = s.status === "outdated";
	if (s.status === "upgraded" && !toasted) {
		toasted = true;
		toast.info("Terminal host updated");
	}
}

/** Follow the app's reconciliation of the terminal host version (see `host.rs`). The status is
 * read once too: the daemon is checked at startup, possibly before this listener exists. */
export async function initHostStatus(): Promise<UnlistenFn> {
	const off = await listen<HostStatus>("host://status", (e) => applyHostStatus(e.payload));
	applyHostStatus(await invoke<HostStatus | null>("host_status"));
	return off;
}
