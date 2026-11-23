/**
 * What a failed `term_open` means for the page. Pure (the platform is a parameter, like the other platform rules).
 *
 *  - `hostTooOld`: the running terminal host predates the raw byte stream; terminals use xterm.js until it restarts.
 *  - `fallback`: Linux. The native terminal could not be made (no Vulkan surface, a protocol error, the compositor refusing
 *    the subsurface). Never a blank pane: every terminal uses xterm.js for the rest of the run.
 *  - `toast`: macOS keeps its error toast.
 */
export type OpenFailure = "hostTooOld" | "fallback" | "toast";

export function openFailure(message: string, mac: boolean): OpenFailure {
	if (message.includes("pty_attach_raw")) return "hostTooOld";
	return mac ? "toast" : "fallback";
}
