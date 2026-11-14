/**
 * Everything the desktop writes or runs on a crew server to set an agent engine up: the per-engine env files hostd
 * injects into the agent process (`~/.atlas/agent-env/<agent>.env`), the bring-your-own-provider config files, and
 * the one-shot connection check. The onboarding and the crewmate settings dialog both call this; nothing else
 * should assemble these shell scripts.
 *
 * The data dir is `$HOME/.atlas` of the crew's service user (the shell expands it), so it holds for `atlas-crew` and
 * for servers that kept the older `atlas-bot` user alike.
 */
import { invoke } from "@tauri-apps/api/core";
import {
	buildClaudeEnvLines,
	buildCodexConfigToml,
	buildOmpModelsYml,
	buildOpencodeConfig,
	CODEX_PROVIDER_ENV,
	DEFAULT_SUMMARY_MODEL,
	OMP_PROVIDER_ENV,
	OPENCODE_PROVIDER_ENV,
	type ApiFormat,
} from "#lib/crew/providerConfig.ts";

/** hostd's agent-env dir on a crew server, as shell text (never quoted away, so `$HOME` expands). */
const AGENT_ENV_DIR = "$HOME/.atlas/agent-env";

/** How an engine signs in: an API key only, or interactive sign-in with an optional key. */
export const AGENT_AUTH_TYPE: Record<string, "apikey" | "both"> = {
	omp: "apikey",
	claude: "both",
	codex: "both",
	opencode: "both",
};

/** What the key field asks for, per engine. */
export const AGENT_KEY_LABEL: Record<string, string> = {
	omp: "OpenRouter or OpenAI key",
	claude: "Anthropic API key (optional)",
	codex: "OpenAI API key",
	opencode: "Provider API key",
};

/** The env var each engine reads its key from. */
export const AGENT_KEY_ENV: Record<string, string> = {
	omp: "OPENROUTER_API_KEY",
	claude: "ANTHROPIC_API_KEY",
	codex: "OPENAI_API_KEY",
	opencode: "OPENAI_API_KEY",
};

/** The command that starts an engine's interactive sign-in. */
export const AGENT_SIGNIN_CMD: Record<string, string> = {
	claude: "claude",
	codex: "codex",
	opencode: "opencode",
	omp: "omp",
};

/** Run a shell script on a crew server through hostd; resolves with hostd's exec result. */
export function runOnServer(
	hostKey: string,
	script: string,
	opts: { cwd?: string; timeoutMs?: number } = {},
): Promise<unknown> {
	return invoke("host_call", {
		cmd: "exec",
		args: {
			host: hostKey,
			cwd: opts.cwd ?? "/",
			argv: ["sh", "-c", script],
			timeout_ms: opts.timeoutMs ?? 30000,
		},
	});
}

/** Write a text file on a crew server (`path` may start with `$HOME`), mode 0600. */
function writeServerFile(hostKey: string, path: string, content: string): Promise<unknown> {
	return runOnServer(
		hostKey,
		`mkdir -p "$(dirname "${path}")"; ` +
			`printf '%s' '${btoa(content)}' | base64 -d > "${path}"; ` +
			`chmod 600 "${path}"`,
	);
}

/**
 * Write `<agent-env>/<agentId>.env`. The 0700 dir is what keeps the key private: the file is created with the
 * shell's umask 077 and chmod'ed 600 right after (an earlier version left omp.env world-readable).
 */
export async function writeAgentEnv(hostKey: string, agentId: string, content: string): Promise<void> {
	const file = `${AGENT_ENV_DIR}/${agentId}.env`;
	await runOnServer(
		hostKey,
		`umask 077; mkdir -p "${AGENT_ENV_DIR}" && chmod 700 "${AGENT_ENV_DIR}"; ` +
			`printf '%s' '${btoa(content)}' | base64 -d > "${file}"; chmod 600 "${file}"`,
	);
}

/**
 * Store an API key for an engine and, for codex, log it in. Read from the file, never on argv (visible to every
 * user in `ps`).
 */
export async function applyAgentApiKey(hostKey: string, agentId: string, apiKey: string): Promise<void> {
	await writeAgentEnv(hostKey, agentId, `${AGENT_KEY_ENV[agentId] ?? "API_KEY"}=${apiKey.trim()}\n`);
	if (agentId === "codex") {
		await runOnServer(
			hostKey,
			`export PATH="$HOME/.npm-global/bin:$PATH"; v=$(sed -n 's/^[^=]*=//p' "${AGENT_ENV_DIR}/codex.env"); ` +
				`printf '%s' "$v" | codex login --with-api-key`,
		);
	}
}

export interface ProviderInput {
	baseUrl: string;
	apiFormat: ApiFormat;
	model: string;
	apiKey: string;
}

/** Write the engine's config file for a custom gateway and its key to agent-env. */
export async function saveProviderConfig(hostKey: string, agentId: string, p: ProviderInput): Promise<void> {
	const { baseUrl, apiFormat, model, apiKey } = p;
	switch (agentId) {
		case "omp":
			// models.yml references the env var by name; the key itself only goes to agent-env.
			await writeServerFile(
				hostKey,
				"$HOME/.omp/agent/models.yml",
				buildOmpModelsYml({ baseUrl, apiFormat, model, summaryModel: DEFAULT_SUMMARY_MODEL }),
			);
			await writeAgentEnv(hostKey, "omp", `${OMP_PROVIDER_ENV}=${apiKey}\n`);
			break;
		case "claude":
			// ANTHROPIC_BASE_URL + ANTHROPIC_API_KEY in agent-env; no config file.
			await writeAgentEnv(hostKey, "claude", buildClaudeEnvLines({ baseUrl, apiKey }));
			break;
		case "codex":
			await writeServerFile(hostKey, "$HOME/.codex/config.toml", buildCodexConfigToml({ baseUrl, model }));
			await writeAgentEnv(hostKey, "codex", `${CODEX_PROVIDER_ENV}=${apiKey}\n`);
			break;
		case "opencode":
			// opencode's config has no env-var resolution, so the key sits inline in a 0600 file; the agent-env
			// entry is the marker that makes the engine check report signed-in.
			await writeServerFile(
				hostKey,
				"$HOME/.config/opencode/opencode.json",
				buildOpencodeConfig({ baseUrl, apiFormat, model, apiKey }),
			);
			await writeAgentEnv(hostKey, "opencode", `${OPENCODE_PROVIDER_ENV}=${apiKey}\n`);
			break;
	}
}

/** One-shot prompt per engine that proves the configured gateway answers end to end. */
function connectionCheckCommand(agentId: string, model: string): string | null {
	switch (agentId) {
		case "omp":
			return `omp -p "reply with: ok" --model atlas_gateway/${model} 2>&1 | head -c 400`;
		case "claude":
			return `claude -p "reply with: ok" 2>&1 | head -c 400`;
		case "codex":
			return `codex exec --dangerously-bypass-hook-trust "Reply with only the word: ok" 2>&1 | head -c 400`;
		case "opencode":
			return `opencode run --format json --standalone --auto "Reply with only the word: ok" 2>&1 | head -c 400`;
		default:
			return null;
	}
}

/** Run the engine once on the crew server with its agent-env sourced (90 s limit). No-op for unknown engines. */
export async function checkProviderConnection(
	hostKey: string,
	agentId: string,
	model: string,
): Promise<void> {
	const cmd = connectionCheckCommand(agentId, model);
	if (!cmd) return;
	const pathExport = `export PATH="$HOME/.bun/bin:$HOME/.local/bin:$HOME/.npm-global/bin:$HOME/.opencode/bin:$PATH"`;
	const srcEnv = `ef="${AGENT_ENV_DIR}/${agentId}.env"; [ -f "$ef" ] && . "$ef" 2>/dev/null || true`;
	await runOnServer(hostKey, `${pathExport}; ${srcEnv}; ${cmd}`, { cwd: "/tmp", timeoutMs: 90000 });
}

/** Save the provider config and verify it: the one call both the onboarding and the settings dialog make. */
export async function saveAndCheckProvider(
	hostKey: string,
	agentId: string,
	p: ProviderInput,
): Promise<void> {
	await saveProviderConfig(hostKey, agentId, p);
	await checkProviderConnection(hostKey, agentId, p.model);
}

/** Probe a gateway for its model list (no crew server involved: the desktop asks the gateway directly). */
export async function probeProvider(baseUrl: string, apiKey: string): Promise<{ ok: boolean; text: string }> {
	const models = await invoke<string[]>("provider_probe", { baseUrl: baseUrl.trim(), apiKey: apiKey.trim() });
	return {
		ok: true,
		text: models.length
			? `Connected — ${models.length} model${models.length === 1 ? "" : "s"} available`
			: "Connected",
	};
}
