/**
 * The client half of the dictation commands of hostd (`crew_voice_status`, `crewmate_voice_put`, `crewmate_transcribe`),
 * read defensively: a field an older hostd does not send reads as its default (voice off), and an unknown field is
 * ignored. The shape is `apps/desktop/src/lib/__fixtures__/crew-voice.json`, written by hostd's serializer.
 */

/** The engine as `crew_voice_status` reports it. */
export interface VoiceEngine {
	kind: "whisper_cpp" | "openai_compatible";
	/** What the apps show verbatim next to the microphone. */
	label: string;
	model: string;
	/** Installed and runnable right now. */
	ready: boolean;
	/** Why not ready, a short English line for logs. */
	detail: string | null;
}

/** What a crew server can do with a voice. */
export interface VoiceStatus {
	/** The crew-wide switch (the owner's): off, every transcription is refused. */
	enabled: boolean;
	engine: VoiceEngine | null;
	maxSeconds: number;
}

/** A crew server that predates dictation (or answered nothing usable): voice is off. */
export const VOICE_OFF: VoiceStatus = { enabled: false, engine: null, maxSeconds: 300 };

const isObject = (v: unknown): v is Record<string, unknown> =>
	!!v && typeof v === "object" && !Array.isArray(v);

/** The `crew_voice_status` reply (also `crew_voice_set`'s). */
export function parseVoiceStatus(raw: unknown): VoiceStatus {
	if (!isObject(raw)) return VOICE_OFF;
	const e = raw.engine;
	const engine =
		isObject(e) && typeof e.label === "string" && e.label
			? {
					kind: e.kind === "openai_compatible" ? ("openai_compatible" as const) : ("whisper_cpp" as const),
					label: e.label,
					model: typeof e.model === "string" ? e.model : "",
					ready: e.ready === true,
					detail: typeof e.detail === "string" ? e.detail : null,
				}
			: null;
	return {
		enabled: raw.enabled === true,
		engine,
		maxSeconds: typeof raw.max_seconds === "number" && raw.max_seconds > 0 ? raw.max_seconds : 300,
	};
}

/** What `crewmate_transcribe` returns. */
export interface Transcript {
	/** After the spoken-symbol replacements: what goes into the composer. */
	text: string;
	raw: string;
	/** The engine that actually ran (its label is what the person sees). */
	engineLabel: string;
	partial: boolean;
}

export function parseTranscript(raw: unknown): Transcript {
	const o = isObject(raw) ? raw : {};
	const e = isObject(o.engine) ? o.engine : {};
	return {
		text: typeof o.text === "string" ? o.text : "",
		raw: typeof o.raw === "string" ? o.raw : "",
		engineLabel: typeof e.label === "string" ? e.label : "",
		partial: o.partial === true,
	};
}
