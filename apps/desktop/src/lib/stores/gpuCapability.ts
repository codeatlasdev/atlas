/**
 * What `term_capability` (Rust, `term/linux/capability.rs`) says, turned into the page's switches. Pure: the store
 * (`gpu.svelte.ts`) calls it, the tests feed it the reply the real producer serializes.
 *
 * macOS has the native terminal always and decides nothing here. Linux decides in Rust from the session (Wayland, not
 * X11 or XWayland), the compositor, the GPU and the setting; the main window is made see-through at launch only when
 * the setting was on, so the terminal is in use only when the setting was on AT LAUNCH (`active`), and a change made
 * now needs a restart.
 */

export interface Capability {
	/** The machine can draw the native terminal: the Settings toggle is offered. */
	available: boolean;
	/** Why not, when it cannot (the log has it too). */
	reason: string | null;
	adapter: string | null;
	/** The setting as saved now. */
	setting: boolean;
	/** In use in this run: available, asked for, and the window was made see-through at launch. */
	active: boolean;
}

export interface GpuSwitches {
	/** Terminals render natively now. */
	supported: boolean;
	/** The Settings toggle is shown. */
	offered: boolean;
	/** The user's choice (the toggle). */
	terminal: boolean;
	/** The choice differs from what this run started with: a restart applies it. */
	restartNeeded: boolean;
	reason: string | null;
}

/** The switches for a Linux reply. A machine that cannot offers nothing and never renders natively. */
export function fromCapability(c: Capability): GpuSwitches {
	return {
		supported: c.active,
		offered: c.available,
		terminal: c.available && c.setting,
		restartNeeded: c.available && c.setting !== c.active,
		reason: c.reason,
	};
}

/** After the user flips the toggle: the same reply with the new setting, `active` unchanged (it is a launch fact). */
export function withSetting(c: Capability, on: boolean): Capability {
	return { ...c, setting: on };
}
