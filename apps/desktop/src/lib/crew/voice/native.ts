/** The shell's microphone (`crates/atlas-desktop/src/voice`) as the `Native` of `Dictation`: Tauri commands and events. */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { b64bytes } from "#lib/b64.ts";
import type { Native, Take } from "./dictation.svelte.ts";

export const nativeMic: Native = {
	start: () => invoke("voice_start"),
	stop: () => invoke<Take | null>("voice_stop"),
	cancel: () => invoke("voice_cancel"),
	async read(offset, max) {
		const p = await invoke<{ data_b64: string; total: number; live: boolean }>("voice_read", { offset, max });
		return { data: b64bytes(p.data_b64), total: p.total, live: p.live };
	},
	openSettings: () => invoke("voice_open_settings"),
	onLevel: (cb) => listen<number>("voice://level", (e) => cb(e.payload)),
	onStopped: (cb) => listen<Take>("voice://stopped", (e) => cb(e.payload)),
};
