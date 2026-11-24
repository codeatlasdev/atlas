import { invoke } from "@tauri-apps/api/core";
import { toast } from "@codeatlasdev/ui";
import { IS_MAC } from "#lib/keys.ts";
import { m } from "#lib/i18n/m.ts";
import { gpu, GPU_KEY } from "./gpu.svelte.ts";
import { fromCapability, refusal, withSetting, type Capability } from "./gpuCapability.ts";

/**
 * Asking Rust what the machine can do and saving the choice. Kept out of `gpu.svelte.ts` (the plain state every terminal
 * reads at startup) so the capability code loads lazily: the session imports this after first paint, and the Settings
 * tab with its toggle is lazy already (bundle budget, `bun run bundle`).
 */

let capability: Capability | null = null;

function apply(c: Capability) {
	capability = c;
	const s = fromCapability(c);
	gpu.supported = s.supported;
	gpu.offered = s.offered;
	gpu.terminal = s.terminal;
	gpu.restartNeeded = s.restartNeeded;
}

/** Ask Rust what this machine can do (Linux; macOS has it always and Windows never). Resolves with the capability. */
export async function detectGpu(): Promise<Capability | null> {
	if (IS_MAC) return null;
	try {
		const c = await invoke<Capability>("term_capability");
		apply(c);
		// Asked for and the machine cannot: every terminal is xterm.js, and the user is told why (once per launch).
		const why = refusal(c);
		if (why) toast.info(m.workbench_term_native_fell_back({ reason: why }));
		return c;
	} catch {
		return null;
	}
}

export function setGpuTerminal(on: boolean) {
	if (IS_MAC) {
		gpu.terminal = on;
		localStorage.setItem(GPU_KEY, on ? "1" : "0");
		return;
	}
	if (!capability) return;
	apply(withSetting(capability, on));
	void invoke("settings_patch", { patch: { gpuTerminal: on } }).catch(() => {});
}
