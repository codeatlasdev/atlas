/**
 * Crew-level settings of one crew server (`crew_settings_get` / `crew_settings_set`): the short-replies switch and
 * its summary model. Shared by the Settings page (per server) and the crewmate dialog (the "Crew setting" label).
 */
import { hostCall } from "#lib/host.ts";
import { DEFAULT_SUMMARY_MODEL } from "#lib/crew/providerConfig.ts";

export interface CrewServerSettings {
	short_replies: boolean;
	short_replies_model: string;
}

/** Read a crew server's crew-level settings; a server that never set a model reports the default one. */
export async function loadCrewSettings(hostKey: string): Promise<CrewServerSettings> {
	const r = await hostCall<{ short_replies: boolean; short_replies_model: string | null }>(
		"crew_settings_get",
		{
			host: hostKey,
		},
	);
	return {
		short_replies: r.short_replies,
		short_replies_model: r.short_replies_model ?? DEFAULT_SUMMARY_MODEL,
	};
}

/** Write a crew server's crew-level settings; an empty model clears it (hostd falls back to the default). */
export async function saveCrewSettings(hostKey: string, s: CrewServerSettings): Promise<void> {
	await hostCall("crew_settings_set", {
		host: hostKey,
		short_replies: s.short_replies,
		short_replies_model: s.short_replies_model || null,
	});
}
