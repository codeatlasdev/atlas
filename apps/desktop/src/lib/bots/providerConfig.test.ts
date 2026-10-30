/**
 * Tests for providerConfig.ts — config builders for custom gateway providers.
 *
 * Contract: given base URL, API format, model, and (for opencode) an API key,
 * each builder returns the exact file content to write on the ship.
 *
 * The API key MUST NOT appear inline in omp/codex/claude output (env-var
 * reference only); it MAY appear inline in opencode output (no env-var
 * resolution in that format).
 *
 * Guards: custom-provider UI — ensures the file shapes shipped to the bot
 * server never regress.
 */
// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, test, expect } from "bun:test";
import {
	buildOmpModelsYml,
	buildClaudeEnvLines,
	buildCodexConfigToml,
	buildOpencodeConfig,
	OMP_PROVIDER_ENV,
	CODEX_PROVIDER_ENV,
	OPENCODE_PROVIDER_ENV,
	PROVIDER_ID,
	type ApiFormat,
} from "./providerConfig.ts";

const BASE = "https://openrouter.ai/api/v1";
const MODEL = "anthropic/claude-opus-4-5";
const KEY = "sk-or-test-key";

// ─── omp models.yml ───────────────────────────────────────────────────────────

describe("buildOmpModelsYml", () => {
	test("OpenAI format uses openai-completions wire", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain("api: openai-completions");
	});

	test("Anthropic format uses anthropic-messages wire", () => {
		const yaml = buildOmpModelsYml({
			baseUrl: BASE,
			apiFormat: "anthropic",
			model: MODEL,
		});
		expect(yaml).toContain("api: anthropic-messages");
	});

	// Incident: short replies never happened on the test ship: the provider only listed the bot's model, omp refused the
	// cheap summary model ("cc/claude-haiku-5-5"), and every reply fell back to the long original.
	test("also registers the short-replies model, once", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "anthropic", model: MODEL, summaryModel: "cc/claude-haiku-5-5" });
		expect(yaml).toContain(`- id: ${MODEL}`);
		expect(yaml).toContain("- id: cc/claude-haiku-5-5");
		const same = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "anthropic", model: MODEL, summaryModel: MODEL });
		expect(same.split(`- id: ${MODEL}`).length - 1).toBe(1);
	});

	test("baseUrl written verbatim", () => {
		const url = "https://my-llm.corp.example/v1";
		const yaml = buildOmpModelsYml({ baseUrl: url, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain(`baseUrl: ${url}`);
	});

	test("model id written as-is", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain(`id: ${MODEL}`);
		expect(yaml).toContain(`name: ${MODEL}`);
	});

	test("apiKey references env-var name, never literal key", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain(`apiKey: ${OMP_PROVIDER_ENV}`);
		// No accidental literal key value
		expect(yaml).not.toContain("sk-");
		expect(yaml).not.toContain("Bearer");
	});

	test("provider block is under PROVIDER_ID key", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain(`  ${PROVIDER_ID}:`);
	});

	test("models block has contextWindow and maxTokens", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		expect(yaml).toContain("contextWindow:");
		expect(yaml).toContain("maxTokens:");
	});
});

// ─── claude agent-env lines ───────────────────────────────────────────────────

describe("buildClaudeEnvLines", () => {
	test("writes ANTHROPIC_BASE_URL and ANTHROPIC_API_KEY", () => {
		const lines = buildClaudeEnvLines({ baseUrl: BASE, apiKey: KEY });
		expect(lines).toContain(`ANTHROPIC_BASE_URL=${BASE}`);
		expect(lines).toContain(`ANTHROPIC_API_KEY=${KEY}`);
	});

	test("each assignment on its own line", () => {
		const lines = buildClaudeEnvLines({ baseUrl: BASE, apiKey: KEY });
		const parts = lines.split("\n").filter(Boolean);
		expect(parts).toHaveLength(2);
		expect(parts[0]).toMatch(/^ANTHROPIC_BASE_URL=/);
		expect(parts[1]).toMatch(/^ANTHROPIC_API_KEY=/);
	});

	test("URL written verbatim", () => {
		const url = "https://litellm.internal:4000/v1";
		const lines = buildClaudeEnvLines({ baseUrl: url, apiKey: KEY });
		expect(lines).toContain(url);
	});
});

// ─── codex config.toml ───────────────────────────────────────────────────────

describe("buildCodexConfigToml", () => {
	test("sets model and model_provider at top level", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).toContain(`model = "${PROVIDER_ID}/${MODEL}"`);
		expect(toml).toContain(`model_provider = "${PROVIDER_ID}"`);
	});

	test("provider block uses PROVIDER_ID", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).toContain(`[model_providers.${PROVIDER_ID}]`);
	});

	test("base_url written verbatim", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).toContain(`base_url = "${BASE}"`);
	});

	test("env_key references CODEX_PROVIDER_ENV, never literal key", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).toContain(`env_key = "${CODEX_PROVIDER_ENV}"`);
		expect(toml).not.toContain("sk-");
	});

	test("wire_api is responses (only supported value)", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).toContain(`wire_api = "responses"`);
	});
});

// ─── opencode config ─────────────────────────────────────────────────────────

describe("buildOpencodeConfig", () => {
	test("output is valid JSON", () => {
		const json = buildOpencodeConfig({
			baseUrl: BASE,
			apiFormat: "openai",
			model: MODEL,
			apiKey: KEY,
		});
		expect(() => JSON.parse(json)).not.toThrow();
	});

	test("provider block nested under PROVIDER_ID", () => {
		const config = JSON.parse(
			buildOpencodeConfig({ baseUrl: BASE, apiFormat: "openai", model: MODEL, apiKey: KEY }),
		);
		expect(config.provider).toBeDefined();
		expect(config.provider[PROVIDER_ID]).toBeDefined();
	});

	test("OpenAI format selects openai-compatible npm package", () => {
		const config = JSON.parse(
			buildOpencodeConfig({ baseUrl: BASE, apiFormat: "openai", model: MODEL, apiKey: KEY }),
		);
		expect(config.provider[PROVIDER_ID].npm).toBe("@ai-sdk/openai-compatible");
	});

	test("Anthropic format selects anthropic npm package", () => {
		const config = JSON.parse(
			buildOpencodeConfig({
				baseUrl: BASE,
				apiFormat: "anthropic",
				model: MODEL,
				apiKey: KEY,
			}),
		);
		expect(config.provider[PROVIDER_ID].npm).toBe("@ai-sdk/anthropic");
	});

	test("baseURL and apiKey written into options", () => {
		const config = JSON.parse(
			buildOpencodeConfig({ baseUrl: BASE, apiFormat: "openai", model: MODEL, apiKey: KEY }),
		);
		const opts = config.provider[PROVIDER_ID].options;
		expect(opts.baseURL).toBe(BASE);
		expect(opts.apiKey).toBe(KEY);
	});

	test("model entry in models map", () => {
		const config = JSON.parse(
			buildOpencodeConfig({ baseUrl: BASE, apiFormat: "openai", model: MODEL, apiKey: KEY }),
		);
		expect(config.provider[PROVIDER_ID].models[MODEL]).toBeDefined();
		expect(config.provider[PROVIDER_ID].models[MODEL].name).toBe(MODEL);
	});

	test("key IS inline (opencode has no env-var resolution)", () => {
		// This test documents the known limitation: opencode embeds the key.
		const json = buildOpencodeConfig({
			baseUrl: BASE,
			apiFormat: "openai",
			model: MODEL,
			apiKey: KEY,
		});
		expect(json).toContain(KEY);
	});
});

// ─── cross-engine invariants ──────────────────────────────────────────────────

describe("cross-engine key-inline invariants", () => {
	test("omp output never contains a literal key prefix", () => {
		const yaml = buildOmpModelsYml({ baseUrl: BASE, apiFormat: "openai", model: MODEL });
		// Guard: if someone changes the builder to embed the key these must fail
		expect(yaml).not.toContain("sk-or-");
		expect(yaml).not.toContain("key-");
	});

	test("codex output never contains a literal key prefix", () => {
		const toml = buildCodexConfigToml({ baseUrl: BASE, model: MODEL });
		expect(toml).not.toContain("sk-");
		expect(toml).not.toContain("key-");
	});
});
