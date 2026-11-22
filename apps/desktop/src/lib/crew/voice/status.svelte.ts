/**
 * What each crew server says about dictation (`crew_voice_status`), loaded when a crewmate is selected and when the
 * settings page opens. A server that predates dictation rejects the command: its entry stays `null` ("unknown"), so the
 * composer shows the microphone off with the reason instead of guessing.
 */
import { hostCall } from "#lib/host.ts";
import { parseVoiceStatus, type VoiceStatus } from "./wire.ts";

const route = (hostKey: string) => (hostKey ? { host: hostKey } : {});

class VoiceStatusStore {
	/** Last status per host key ("" is this computer). `null`: the server cannot do dictation (or did not answer). */
	byHost = $state<Record<string, VoiceStatus | null>>({});
	/** Hosts whose first answer has arrived (a `null` entry then means "unsupported", not "not asked yet"). */
	loaded = $state<Record<string, true>>({});

	of(hostKey: string): VoiceStatus | null {
		return this.byHost[hostKey] ?? null;
	}

	async load(hostKey: string): Promise<void> {
		let next: VoiceStatus | null = null;
		try {
			next = parseVoiceStatus(await hostCall("crew_voice_status", route(hostKey)));
		} catch {
			// An older crew server does not know the command.
		}
		this.byHost = { ...this.byHost, [hostKey]: next };
		this.loaded = { ...this.loaded, [hostKey]: true };
	}

	/** The owner's `crew_voice_set`; the reply is the new status. A refusal (not the owner) is thrown, coded. */
	async set(hostKey: string, patch: { enabled?: boolean }): Promise<void> {
		const r = await hostCall("crew_voice_set", { ...route(hostKey), ...patch });
		this.byHost = { ...this.byHost, [hostKey]: parseVoiceStatus(r) };
	}
}

export const voiceStatus = new VoiceStatusStore();
