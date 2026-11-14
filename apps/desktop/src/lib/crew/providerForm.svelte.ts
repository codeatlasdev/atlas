/**
 * The "use your own provider" form (gateway URL, API format, key, model) with its connection test and save, shared by
 * the onboarding engine step and the crewmate settings Engine section. Saving writes files on the crew server and runs
 * a connection check (`remoteAgent.ts`); what happens after a save (engine Ready, a toast) is the caller's.
 */
import { errText } from "#lib/api.ts";
import type { ApiFormat } from "#lib/crew/providerConfig.ts";
import { probeProvider, saveAndCheckProvider } from "#lib/crew/remoteAgent.ts";

/** Engines whose provider can be rerouted through a custom gateway. */
export const PROVIDER_AGENTS = ["omp", "claude", "codex", "opencode"];

const DEFAULT_BASE_URL = "https://openrouter.ai/api/v1";

export class ProviderForm {
	baseUrl = $state(DEFAULT_BASE_URL);
	format = $state<ApiFormat>("openai");
	apiKey = $state("");
	model = $state("");
	testing = $state(false);
	testResult = $state<{ ok: boolean; text: string } | null>(null);
	saving = $state(false);
	saveError = $state("");

	/** Test needs a URL and a key. */
	get canTest() {
		return !this.testing && !!this.baseUrl.trim() && !!this.apiKey.trim();
	}

	/** Save needs a URL, a key and a model. */
	get canSave() {
		return !this.saving && !!this.baseUrl.trim() && !!this.model.trim() && !!this.apiKey.trim();
	}

	/** Forget the last test and save error (switching engines or reopening the form). */
	resetFeedback() {
		this.testResult = null;
		this.saveError = "";
	}

	/** Back to a blank form (nothing is read back from the crewmate record). */
	reset() {
		this.baseUrl = DEFAULT_BASE_URL;
		this.format = "openai";
		this.apiKey = "";
		this.model = "";
		this.resetFeedback();
	}

	async test() {
		if (!this.canTest) return;
		this.testing = true;
		this.testResult = null;
		try {
			this.testResult = await probeProvider(this.baseUrl, this.apiKey);
		} catch (e) {
			this.testResult = { ok: false, text: errText(e) };
		} finally {
			this.testing = false;
		}
	}

	/** Write the provider config for `agentId` on crew server `hostKey` and check it answers. True when it does; the
	 * key is then cleared from the form. On failure the reason is in `saveError`. */
	async save(hostKey: string | undefined, agentId: string | undefined): Promise<boolean> {
		if (!hostKey || !agentId || !this.canSave) return false;
		this.saving = true;
		this.saveError = "";
		try {
			await saveAndCheckProvider(hostKey, agentId, {
				baseUrl: this.baseUrl.trim(),
				apiFormat: this.format,
				model: this.model.trim(),
				apiKey: this.apiKey.trim(),
			});
			this.apiKey = "";
			return true;
		} catch (e) {
			this.saveError = errText(e);
			return false;
		} finally {
			this.saving = false;
		}
	}
}
