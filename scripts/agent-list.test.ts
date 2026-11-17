// Incident class: the built-in agent list was typed nine times (Rust, TS, Dart) and the copies disagreed on
// OpenCode. crates/atlas-core/data/agents.json is the only list; production code reads it. This fails when a
// list of two or more agent ids appears anywhere else, and says where the list belongs.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const ID = `["'\`](?:omp|claude|codex|opencode)["'\`]`;
// Two agent ids in one bracketed list, in any order: ["omp", "claude"], ['claude', 'codex'], &["codex", "omp"].
const LIST = new RegExp(`\\[[^\\]\\[]*${ID}\\s*,\\s*${ID}[^\\]\\[]*\\]`);
const SCAN = ["crates", "apps/desktop/src", "apps/mobile/lib", "site/src", "services"];
const SKIP = /(?:^|\/)(?:target|node_modules|\.svelte-kit|build|testdata|__fixtures__|fixtures|tests?)(?:\/|$)|(?:\.test\.ts|_test\.dart|tests\.rs|_tests\.rs|agent_registry_data\.dart)$|agents\.json$/;

function files(): string[] {
	const out = Bun.spawnSync(["git", "ls-files", "-z", "--", ...SCAN], { cwd: ROOT }).stdout.toString();
	return out
		.split("\0")
		.filter((f) => /\.(rs|ts|svelte|dart)$/.test(f) && !SKIP.test(f));
}

/** The production part of a file: Rust unit tests (`#[cfg(test)]` and everything after it) are not. */
function production(path: string): string {
	const text = readFileSync(join(ROOT, path), "utf8");
	if (!path.endsWith(".rs")) return text;
	const at = text.search(/^#\[cfg\(test\)\]/m);
	return at < 0 ? text : text.slice(0, at);
}

test("no production file lists two or more built-in agents: the one list is crates/atlas-core/data/agents.json", () => {
	const hits: string[] = [];
	for (const f of files()) {
		const lines = production(f).split("\n");
		lines.forEach((line, i) => {
			if (LIST.test(line)) hits.push(`${f}:${i + 1}: ${line.trim()}`);
		});
	}
	expect(
		hits,
		"a list of agent ids was typed again. Read it from the registry instead: Rust `atlas_core::agents::all()`, " +
			"desktop `BUILTIN_AGENTS` (lib/agentRegistry.ts), mobile `builtinAgents` (core/protocol/agent_registry.dart). " +
			"A new agent is one entry in crates/atlas-core/data/agents.json, then `bun scripts/gen-agents-dart.ts`.",
	).toEqual([]);
});

test("the guard itself catches the shapes the old copies had (and not unrelated brackets)", () => {
	for (const bad of [
		'["omp", "claude", "codex"]',
		`['claude', 'codex', 'opencode', 'omp']`,
		'let a = ["codex", "omp"].into_iter();',
		"const _agents = ['claude', 'codex'];",
	]) {
		expect(LIST.test(bad), bad).toBe(true);
	}
	for (const ok of ['["omp"]', 'm.get("claude")', '["claude-code", "x"]', '"omp", "claude"', 'f(["a", "b"], "omp")']) {
		expect(LIST.test(ok), ok).toBe(false);
	}
});
