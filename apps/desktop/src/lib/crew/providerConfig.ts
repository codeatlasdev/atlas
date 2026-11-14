/**
 * Per-engine config builders for custom provider (bring-your-own gateway).
 *
 * Each function receives the gateway coordinates and returns the exact text to
 * write to the config file on the crew server.  The API key is NEVER written inline
 * for engines that support env-var references (omp, codex, claude); only
 * opencode embeds the key because its config format has no env-var resolution.
 *
 * Guards: Atlas Crew provider UI — incident: user had no way to point an engine
 * at their own OpenRouter / LiteLLM gateway from the UI.
 */

export type ApiFormat = "openai" | "anthropic";

/** Provider ID written into every config file. */
export const PROVIDER_ID = "atlas_gateway";

/** Env-var name that holds the gateway API key for omp.
 *  Written to the agent-env file; referenced by name in models.yml so the key
 *  never appears in the YAML. */
export const OMP_PROVIDER_ENV = "ATLAS_OMP_PROVIDER_KEY";

/** Env-var name that holds the gateway API key for codex.
 *  Written to the agent-env file; referenced via env_key in config.toml. */
export const CODEX_PROVIDER_ENV = "ATLAS_CODEX_PROVIDER_KEY";

/** Env-var name written to agent-env as a "provider configured" marker for
 *  opencode (the real key is also embedded inline in opencode.json because
 *  opencode's config format has no env-var resolution). */
export const OPENCODE_PROVIDER_ENV = "ATLAS_OPENCODE_PROVIDER_KEY";

// ─── omp ─────────────────────────────────────────────────────────────────────

/**
 * Build `~/.omp/agent/models.yml` for a custom gateway.
 *
 * `apiKey` is the env-var NAME (OMP_PROVIDER_ENV), never the literal key.
 * omp resolves the value at startup: if the named var is set in the process
 * env it uses that; otherwise it falls back to treating the string as a
 * literal key.  The agent-env loader in runner.rs injects it before omp runs.
 *
 * The `api` field selects the wire protocol:
 *   "openai"    → openai-completions  (Chat Completions /v1/chat/completions)
 *   "anthropic" → anthropic-messages  (Anthropic Messages API)
 */
/** Short-replies model when the crew has not picked one; must match `default_cheap_model` in
 * crates/atlas-host/src/crew/runner.rs, which runs it. */
export const DEFAULT_SUMMARY_MODEL = "cc/claude-haiku-5-5";

export function buildOmpModelsYml(opts: {
	baseUrl: string;
	apiFormat: ApiFormat;
	model: string;
	/** The cheap model used for short replies; omp only runs models listed here. */
	summaryModel?: string;
}): string {
	const api = opts.apiFormat === "anthropic" ? "anthropic-messages" : "openai-completions";
	const ids = [...new Set([opts.model, opts.summaryModel].filter((m): m is string => !!m))];
	return (
		`providers:\n` +
		`  ${PROVIDER_ID}:\n` +
		`    baseUrl: ${opts.baseUrl}\n` +
		`    api: ${api}\n` +
		`    apiKey: ${OMP_PROVIDER_ENV}\n` +
		`    models:\n` +
		ids
			.map(
				(id) =>
					`      - id: ${id}\n        name: ${id}\n        contextWindow: 128000\n        maxTokens: 8192\n`,
			)
			.join("")
	);
}

// ─── claude ───────────────────────────────────────────────────────────────────

/**
 * Build the agent-env content for claude with a custom Anthropic-compatible
 * gateway.  Claude Code reads ANTHROPIC_BASE_URL and ANTHROPIC_API_KEY (or
 * ANTHROPIC_AUTH_TOKEN) from its process environment; these two lines in the
 * agent-env file are injected by runner.rs before claude runs.
 *
 * Note: Claude Code only speaks the Anthropic Messages protocol; the apiFormat
 * is always "anthropic" for this engine — the caller is expected to enforce
 * this.  The key is written directly to the known Claude env var (not via an
 * intermediate alias) because there is no config file that references it.
 */
export function buildClaudeEnvLines(opts: { baseUrl: string; apiKey: string }): string {
	return `ANTHROPIC_BASE_URL=${opts.baseUrl}\nANTHROPIC_API_KEY=${opts.apiKey}\n`;
}

// ─── codex ───────────────────────────────────────────────────────────────────

/**
 * Build `~/.codex/config.toml` for a custom gateway.
 *
 * Codex only supports the OpenAI Responses wire API (wire_api = "responses"),
 * not Chat Completions.  A gateway must speak /v1/responses for this to work.
 *
 * env_key references CODEX_PROVIDER_ENV (never the literal key).  The model
 * selector is written as "<PROVIDER_ID>/<model>" so codex knows which provider
 * block to use.
 */
export function buildCodexConfigToml(opts: { baseUrl: string; model: string }): string {
	return (
		`model = "${PROVIDER_ID}/${opts.model}"\n` +
		`model_provider = "${PROVIDER_ID}"\n` +
		`\n` +
		`[model_providers.${PROVIDER_ID}]\n` +
		`name = "Custom Gateway"\n` +
		`base_url = "${opts.baseUrl}"\n` +
		`env_key = "${CODEX_PROVIDER_ENV}"\n` +
		`wire_api = "responses"\n`
	);
}

// ─── opencode ─────────────────────────────────────────────────────────────────

/**
 * Build `~/.config/opencode/opencode.json` for a custom gateway.
 *
 * opencode's config has no env-var resolution for apiKey, so the key is
 * embedded inline.  The caller must write this file with mode 0600.
 *
 * npm package selection:
 *   "openai"    → @ai-sdk/openai-compatible  (Chat Completions)
 *   "anthropic" → @ai-sdk/anthropic           (Anthropic Messages)
 */
export function buildOpencodeConfig(opts: {
	baseUrl: string;
	apiFormat: ApiFormat;
	model: string;
	apiKey: string;
}): string {
	const npm = opts.apiFormat === "anthropic" ? "@ai-sdk/anthropic" : "@ai-sdk/openai-compatible";
	const config = {
		provider: {
			[PROVIDER_ID]: {
				npm,
				name: "Custom Gateway",
				models: {
					[opts.model]: {
						name: opts.model,
						limit: { context: 128000, output: 8192 },
					},
				},
				options: {
					baseURL: opts.baseUrl,
					apiKey: opts.apiKey,
				},
			},
		},
	};
	return JSON.stringify(config, null, 2) + "\n";
}
