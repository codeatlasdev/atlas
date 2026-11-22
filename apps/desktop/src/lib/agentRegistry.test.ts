// @ts-nocheck bun test runtime types are not part of the app tsconfig
/**
 * Contract with Rust: `crates/atlas-core/data/agents.json` is the file `atlas_core::agents` embeds (its own test
 * proves the parsed Rust list serializes back to exactly this file), and the file this module imports. These
 * tests pin what the frontend reads off it.
 */
import { expect, test } from "bun:test";
import {
	BUILTIN_AGENTS,
	builtinAgent,
	canWriteCommits,
	isBuiltinAgent,
	resumeCommand,
	type BuiltinAgent,
} from "./agentRegistry.ts";

const KEYS: (keyof BuiltinAgent)[] = [
	"ask_args",
	"bin",
	"can_ask",
	"id",
	"install",
	"key_env",
	"label",
	"one_shot",
	"prompt_flag",
	"resume",
	"sessions_dir",
	"setup",
	"yolo_args",
];

test("every entry has exactly the fields the BuiltinAgent type reads, with the right kinds", () => {
	for (const a of BUILTIN_AGENTS) {
		expect(Object.keys(a).sort()).toEqual(KEYS);
		for (const k of ["id", "label", "bin", "setup", "install", "key_env"]) expect(a[k]).toBeString();
		expect(a.resume.some((t) => t.includes("{session}"))).toBe(true);
		expect(typeof a.can_ask).toBe("boolean");
		expect(a.one_shot === null || a.one_shot[0] === a.bin).toBe(true);
	}
	expect(new Set(BUILTIN_AGENTS.map((a) => a.id)).size).toBe(BUILTIN_AGENTS.length);
});

test("OpenCode is built in, with hooks, but is not offered for commit messages (no verified one-shot run)", () => {
	expect(builtinAgent("opencode")).toMatchObject({
		label: "OpenCode",
		setup: "opencode",
		prompt_flag: "--prompt",
	});
	expect(isBuiltinAgent("opencode")).toBe(true);
	expect(canWriteCommits("opencode")).toBe(false);
	expect(BUILTIN_AGENTS.filter((a) => canWriteCommits(a.id)).map((a) => a.id)).toEqual([
		"omp",
		"claude",
		"codex",
	]);
});

test("a custom or missing agent is not built in and cannot write commits", () => {
	for (const id of ["my-agent", "", undefined]) {
		expect(isBuiltinAgent(id)).toBe(false);
		expect(canWriteCommits(id)).toBe(false);
	}
});

test("the resume line follows each agent's own resume argv", () => {
	expect(resumeCommand("codex", "S1")).toBe("codex resume S1");
	expect(resumeCommand("claude", "S1")).toBe("claude --resume S1");
	expect(resumeCommand("omp", "/tmp/s.jsonl")).toBe("omp --resume /tmp/s.jsonl");
	expect(resumeCommand("opencode", "S1")).toBe("opencode --session S1");
	// A session of an agent the registry does not know keeps the old generic form.
	expect(resumeCommand("gemini", "S1")).toBe("gemini --resume S1");
});
