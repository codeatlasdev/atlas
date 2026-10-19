// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { customAgentEntry } from "./customAgents.ts";

const base = { name: "OpenCode", command: "opencode", args: "", resume: "", env: "" };

describe("custom agent form → customAgents entry (#56)", () => {
	test("derives a slug id, parses args and resume like a shell, env as KEY=value lines", () => {
		const r = customAgentEntry(
			{ ...base, args: `run --model "gpt 5"`, resume: "--session {session}", env: "FOO=1\n\n BAR = a=b \n" },
			[],
		);
		expect(r).toEqual({
			ok: true,
			entry: {
				id: "opencode",
				label: "OpenCode",
				command: "opencode",
				args: ["run", "--model", "gpt 5"],
				resume: ["--session", "{session}"],
				env: { FOO: "1", BAR: "a=b" },
			},
		});
	});

	test("an id never collides with an agent that exists (built-in or custom)", () => {
		expect(customAgentEntry({ ...base, name: "Claude" }, ["omp", "claude", "codex"]).entry.id).toBe(
			"claude-2",
		);
		expect(customAgentEntry(base, ["opencode", "opencode-2"]).entry.id).toBe("opencode-3");
		expect(customAgentEntry({ ...base, name: "  Gemini CLI!! " }, []).entry.id).toBe("gemini-cli");
	});

	test("explains what is wrong instead of saving a broken agent", () => {
		expect(customAgentEntry({ ...base, name: " " }, []).error).toMatch(/name/i);
		expect(customAgentEntry({ ...base, name: "!!!" }, []).error).toMatch(/letter or digit/i);
		expect(customAgentEntry({ ...base, command: "" }, []).error).toMatch(/command/i);
		expect(customAgentEntry({ ...base, args: `"open` }, []).error).toMatch(/quote/i);
		expect(customAgentEntry({ ...base, env: "NOEQUALS" }, []).error).toMatch(/KEY=value/);
		expect(customAgentEntry({ ...base, env: "ATLAS_AGENT=x" }, []).error).toMatch(/ATLAS_/);
	});
});

// Contract with Rust (`agent::registry::customs`): `__fixtures__/custom-agent.json` is the REAL output of
// `customAgentEntry`, which the Rust test parses and launches. `UPDATE_FIXTURES=1` regenerates it.
test("the saved entry matches the fixture Rust reads", async () => {
	const r = customAgentEntry(
		{
			name: "OpenCode",
			command: "opencode run",
			args: "--model x",
			resume: "--session {session}",
			env: "FOO=1",
		},
		["omp", "claude", "codex"],
	);
	const path = new URL("../__fixtures__/custom-agent.json", import.meta.url).pathname;
	const saved = { customAgents: [r.entry] };
	if (process.env.UPDATE_FIXTURES) await Bun.write(path, JSON.stringify(saved, null, 2) + "\n");
	expect(await Bun.file(path).json()).toEqual(saved);
});
