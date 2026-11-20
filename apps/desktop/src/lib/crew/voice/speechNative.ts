/** The shell's synthesizer (`crates/atlas-desktop/src/speech`) as the `SpeechNative` of `Speech`: Tauri commands and the end event. */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { SpeechNative } from "./speech.svelte.ts";

export const nativeSpeech: SpeechNative = {
	available: () => invoke<boolean>("speech_available"),
	speak: (text, lang) => invoke<number>("speech_speak", { text, lang }),
	stop: () => invoke("speech_stop"),
	onEnded: (cb) => listen<number>("speech://ended", (e) => cb(e.payload)),
};
