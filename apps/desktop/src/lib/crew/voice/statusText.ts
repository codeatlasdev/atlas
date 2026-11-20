import { m } from "#lib/i18n/m.ts";
import type { VoiceStatus } from "./wire.ts";

/**
 * The one line Settings shows for a crew server's voice: what it is doing right now. `status` is null once the server
 * answered that it cannot (an older one), `undefined` before it answered at all. Called inside a template.
 */
export function voiceStatusText(status: VoiceStatus | null | undefined): string {
	if (status === undefined) return m.crew_voice_status_checking();
	if (status === null) return m.crew_voice_status_unsupported();
	if (!status.enabled) return m.crew_voice_status_off();
	if (!status.engine) return m.crew_voice_status_not_installed();
	return status.engine.ready
		? m.crew_voice_status_ready({ engine: status.engine.label })
		: m.crew_voice_status_not_ready({ engine: status.engine.label });
}
