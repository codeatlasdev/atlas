import { IS_MAC } from "#lib/keys.ts";

/** macOS: the native terminal is on unless the user turned it off (per machine). */
export const GPU_KEY = "atlas.gpuTerminal";

export const gpu = $state({
	/** Terminals render natively in this run. macOS: always (an AppKit view drawn through Metal). Linux: only on a
	 * Wayland session that can do it AND with the setting on at launch (the window is made see-through then); see
	 * `gpuCapability.ts`. Elsewhere every terminal uses xterm.js. */
	supported: IS_MAC,
	/** The Settings toggle is shown: macOS, or a Linux machine that can run the native terminal. */
	offered: IS_MAC,
	/** The user's choice. macOS: `atlas.gpuTerminal` in localStorage (on by default). Linux: `gpuTerminal` in settings.json,
	 * which Rust reads before any page exists (off by default in this phase). */
	terminal: IS_MAC && localStorage.getItem(GPU_KEY) !== "0",
	/** Linux: the choice differs from what this run started with; a restart applies it. */
	restartNeeded: false,
	/** The native terminal could not be made or kept alive on this machine (surface creation failed, the surface never
	 * came back): every terminal uses xterm.js until the app restarts. */
	failed: false,
	/** The running terminal host predates the raw byte stream and cannot be replaced without
	 * ending terminals (it outlives app updates): terminals use xterm.js until it restarts.
	 * Kept in step with the host state by `stores/hostStatus.svelte.ts`. */
	hostTooOld: false,
});
// The capability round trip and the toggle live in `gpuControl.ts` (lazy: see there).
