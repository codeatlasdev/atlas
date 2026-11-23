// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { openFailure } from "./openFailure.ts";

// Phase 3 (Linux): the native terminal never leaves a blank pane. An open that fails for any reason other than an old host
// (no Vulkan surface, a protocol error, the compositor refusing the subsurface) sends every terminal to xterm.js for the
// rest of the run, instead of an error toast over an empty slot. macOS keeps its toast.
test("on Linux a failed open falls back to xterm.js, whatever the reason", () => {
	for (const why of [
		"Vulkan surface: VK_ERROR_SURFACE_LOST_KHR",
		"the window has no Wayland surface yet",
		"panicked on the main thread: WaylandSurface failed",
	])
		expect(openFailure(why, false)).toBe("fallback");
});

test("an old host is waited for, on every platform, and is not a failure of the native terminal", () => {
	expect(openFailure("unknown method pty_attach_raw", false)).toBe("hostTooOld");
	expect(openFailure("unknown method pty_attach_raw", true)).toBe("hostTooOld");
});

test("on macOS any other failure is an error toast, as before", () => {
	expect(openFailure("no main window", true)).toBe("toast");
});
