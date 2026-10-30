// @ts-nocheck
// Cross-boundary contract (AGENTS.md): the action cards in BotView read these exact keys per kind. The fixture is the
// real Rust `TaskAction` serialized by crates/atlas-host/src/bots/tests.rs (task_action_fixture_is_real_payload),
// the same file the phone's contract test reads, so a rename on the Rust side turns this red.
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const actions = JSON.parse(
	readFileSync(join(import.meta.dir, "../../../../mobile/test/fixtures/task_actions.json"), "utf8"),
);

/** Keys BotView reads for each kind (BotTask.action in lib/bots.ts). */
const READS: Record<string, string[]> = {
	connect_github: ["reason"],
	provide_secret: ["name", "label", "reason"],
	input: ["label", "reason", "placeholder"],
	choose: ["question", "options"],
	sign_in_engine: ["engine"],
};

describe("TaskAction wire shape", () => {
	test("every kind the desktop renders is produced by hostd", () => {
		expect(actions.map((a) => a.kind).sort()).toEqual(Object.keys(READS).sort());
	});

	for (const [kind, keys] of Object.entries(READS)) {
		test(`${kind} carries the keys its card reads`, () => {
			const a = actions.find((x) => x.kind === kind);
			for (const k of keys) expect(a).toHaveProperty(k);
		});
	}

	test("choose options are plain strings (rendered as A, B, C…)", () => {
		const c = actions.find((x) => x.kind === "choose");
		expect(c.options.every((o) => typeof o === "string")).toBe(true);
	});
});
