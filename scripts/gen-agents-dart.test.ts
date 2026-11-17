// Incident class: the built-in agent list was typed nine times (Rust, TS, Dart) and the copies disagreed on
// OpenCode. agents.json is the one source; Atlas Mobile's Dart copy is generated, and a stale or hand-edited
// copy must be a red `bun run check`, with the command that fixes it in the message.
import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { FIX, OUTPUT, ROOT, SOURCE, committed, drift, load, render } from "./gen-agents-dart.ts";

test("the committed Dart registry is what agents.json renders to", () => {
	expect(SOURCE).toBe("crates/atlas-core/data/agents.json");
	expect(drift()).toEqual([]);
});

test("a new agent in agents.json changes the output, so a stale Dart copy is detected", () => {
	const agents = load();
	const more = [...agents, { ...agents[0]!, id: "newagent", label: "New Agent" }];
	expect(render(more)).not.toBe(render(agents));
	expect(render(more)).toContain("id: 'newagent'");
});

test("every agent of agents.json is in the Dart file with its capabilities, in order", () => {
	const dart = readFileSync(join(ROOT, OUTPUT), "utf8");
	const ids = [...dart.matchAll(/^    id: '([^']+)',$/gm)].map((m) => m[1]);
	expect(ids).toEqual(load().map((a) => a.id));
	// opencode: hooks yes, commit messages no (no verified one-shot run).
	expect(dart).toMatch(/id: 'opencode',[^)]*writesCommits: false/);
	expect(dart).toMatch(/id: 'claude',[^)]*setup: 'claude-code',[^)]*writesCommits: true/);
});

test("a hand-edited or stale Dart file is reported with the command that fixes it", () => {
	const good = committed();
	for (const bad of [good.replace("'Codex'", "'Codex CLI'"), good.replace("writesCommits: false", "writesCommits: true"), ""]) {
		expect(drift(bad)).toEqual([`${OUTPUT} is out of date with ${SOURCE}. Fix: ${FIX}`]);
	}
	expect(drift(good)).toEqual([]);
});

test("--check --json is the machine verdict (exit 0 and ok when clean)", () => {
	const p = Bun.spawnSync(["bun", "scripts/gen-agents-dart.ts", "--check", "--json"], { cwd: ROOT });
	expect(p.exitCode).toBe(0);
	expect(JSON.parse(p.stdout.toString())).toEqual({ ok: true, file: OUTPUT, fix: FIX });
});

test("strings are escaped so a label with a quote or dollar stays valid Dart", () => {
	const a = load()[0]!;
	const out = render([{ ...a, label: "It's $x" }]);
	expect(out).toContain("label: 'It\\'s \\$x'");
});
