// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, describe, expect, mock, test } from "bun:test";

type Call = { cmd: string; args: { host: string; cwd: string; argv: string[]; timeout_ms: number } };
let calls: Call[] = [];

mock.module("@tauri-apps/api/core", () => ({
	invoke: async (name: string, payload: Call) => {
		if (name === "provider_probe") return ["m1", "m2"];
		calls.push(payload);
		return {};
	},
}));

// Dynamic on purpose: mock.module must run before remoteAgent.ts loads, or it binds the real tauri `invoke`.
const { applyAgentApiKey, saveAndCheckProvider, saveProviderConfig, checkProviderConnection, probeProvider } =
	await import("./remoteAgent.ts");

const scripts = () => calls.map((c) => c.args.argv[2]);
const decode = (script: string) => atob(script.match(/printf '%s' '([^']*)'/)![1]);

beforeEach(() => {
	calls = [];
});

describe("applyAgentApiKey", () => {
	test("writes the key into the user's agent-env dir, 0700 dir and 0600 file, never a hard-coded service user path", async () => {
		await applyAgentApiKey("ssh:atlas-crew@box", "omp", "  sk-secret \n");
		expect(calls).toHaveLength(1);
		const [script] = scripts();
		expect(calls[0].args.host).toBe("ssh:atlas-crew@box");
		expect(script).toContain(
			'umask 077; mkdir -p "$HOME/.atlas/agent-env" && chmod 700 "$HOME/.atlas/agent-env"',
		);
		expect(script).toContain('chmod 600 "$HOME/.atlas/agent-env/omp.env"');
		expect(script).not.toContain("/var/lib/atlas-");
		expect(decode(script)).toBe("OPENROUTER_API_KEY=sk-secret\n");
	});

	test("the key never travels on argv outside the base64 payload, and codex logs in from the file", async () => {
		await applyAgentApiKey("ssh:atlas-bot@box", "codex", "sk-codex");
		expect(calls).toHaveLength(2);
		const login = scripts()[1];
		expect(login).toContain("codex login --with-api-key");
		expect(login).toContain("$HOME/.atlas/agent-env/codex.env");
		expect(login).not.toContain("sk-codex");
		expect(scripts()[0]).not.toContain("sk-codex");
	});
});

describe("saveProviderConfig", () => {
	const p = {
		baseUrl: "https://gw.example/v1",
		apiFormat: "openai" as const,
		model: "gpt-x",
		apiKey: "sk-gw",
	};

	test("omp: models.yml never holds the literal key; the key goes to agent-env", async () => {
		await saveProviderConfig("ssh:atlas-crew@box", "omp", p);
		const [models, env] = scripts();
		expect(models).toContain('"$HOME/.omp/agent/models.yml"');
		expect(decode(models)).not.toContain("sk-gw");
		expect(decode(env)).toContain("sk-gw");
		expect(env).toContain("omp.env");
	});

	test("claude: env file only, no config file", async () => {
		await saveProviderConfig("ssh:atlas-crew@box", "claude", p);
		expect(calls).toHaveLength(1);
		expect(scripts()[0]).toContain("claude.env");
	});

	test("opencode: key inline in a 0600 config plus the agent-env marker", async () => {
		await saveProviderConfig("ssh:atlas-crew@box", "opencode", p);
		const [cfg, env] = scripts();
		expect(cfg).toContain('"$HOME/.config/opencode/opencode.json"');
		expect(cfg).toContain('chmod 600 "$HOME/.config/opencode/opencode.json"');
		expect(decode(cfg)).toContain("sk-gw");
		expect(env).toContain("opencode.env");
	});

	test("an unknown engine writes nothing", async () => {
		await saveProviderConfig("ssh:atlas-crew@box", "mystery", p);
		expect(calls).toHaveLength(0);
	});
});

describe("checkProviderConnection", () => {
	test("sources the engine's agent-env and runs the one-shot with a 90 s limit in /tmp", async () => {
		await checkProviderConnection("ssh:atlas-crew@box", "omp", "gpt-x");
		expect(calls).toHaveLength(1);
		expect(calls[0].args.cwd).toBe("/tmp");
		expect(calls[0].args.timeout_ms).toBe(90000);
		const script = scripts()[0];
		expect(script).toContain('ef="$HOME/.atlas/agent-env/omp.env"');
		expect(script).toContain("--model atlas_gateway/gpt-x");
	});

	test("an unknown engine runs nothing", async () => {
		await checkProviderConnection("ssh:atlas-crew@box", "mystery", "m");
		expect(calls).toHaveLength(0);
	});
});

describe("saveAndCheckProvider", () => {
	test("saves first, then checks", async () => {
		await saveAndCheckProvider("ssh:atlas-crew@box", "claude", {
			baseUrl: "https://gw.example",
			apiFormat: "anthropic",
			model: "m",
			apiKey: "k",
		});
		expect(scripts()).toHaveLength(2);
		expect(scripts()[0]).toContain("claude.env");
		expect(scripts()[1]).toContain("claude -p");
	});
});

describe("probeProvider", () => {
	test("reports the model count", async () => {
		expect(await probeProvider(" https://gw ", " k ")).toEqual({
			ok: true,
			text: "Connected — 2 models available",
		});
	});
});
