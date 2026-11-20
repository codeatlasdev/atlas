// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import fx from "../../__fixtures__/crew-voice-answers.json" with { type: "json" };

// The wire of #648 as hostd's own serializer wrote it (`crew-voice-answers.json`, regenerated with UPDATE_FIXTURES=1 on
// the hostd side, never by hand). Every verdict reply goes through the client's parser and renderer; the client
// reads fields hostd really sends.
const { parseVerdict, verdictView } = await import("./answers.ts");
const { nextSpoken, newWatch } = await import("./spokenTasks.ts");

const view = (raw: unknown, kind: "approval" | "choice", heard = "what I said") =>
	verdictView(parseVerdict(raw), kind, heard);

describe("every approve verdict of the fixture", () => {
	test("allow and deny are decided (the card closes); nothing else decides", () => {
		expect(view(fx.approve.allow, "approval").decided).toEqual({ allow: true });
		expect(view(fx.approve.deny, "approval").decided).toEqual({ allow: false });
		for (const k of Object.keys(fx.approve)) {
			if (k === "allow" || k === "deny") continue;
			expect(view(fx.approve[k], "approval").decided).toBeNull();
		}
	});

	test("comment puts hostd's text in the note for a tap to confirm", () => {
		const v = view(fx.approve.comment, "approval");
		expect(v.note).toBe(fx.approve.comment.comment);
		expect(v.decided).toBeNull();
		expect(v.message).toContain(fx.approve.comment.comment);
	});

	test("needs_tap says the reason hostd gave, each in its own words", () => {
		const said = {
			needs_tap_bypass: "Bypass",
			needs_tap_rail: "protected path",
			needs_tap_destructive: "destroy data",
			needs_tap_untrusted: "did not come from you",
		};
		const texts = new Set();
		for (const [k, word] of Object.entries(said)) {
			const v = view(fx.approve[k], "approval");
			expect(v.message).toContain(word);
			expect(v.note).toBeNull();
			texts.add(v.message);
		}
		expect(texts.size).toBe(4);
	});

	test("unclear says so and echoes what was heard", () => {
		const v = view(fx.approve.unclear, "approval", "banana");
		expect(v.decided).toBeNull();
		expect(v.message).toContain("banana");
	});
});

describe("every answer verdict of the fixture", () => {
	test("choice is decided with the option text exactly as the card has it", () => {
		const v = view(fx.answer.choice, "choice");
		expect(v.decided).toEqual({ choice: fx.task_agent_line.action.options[1] });
	});

	test("unclear and a tap-only card decide nothing; the card kind says why", () => {
		expect(view(fx.answer.unclear, "choice").decided).toBeNull();
		const tap = view(fx.answer.needs_tap_card, "choice");
		expect(tap.decided).toBeNull();
		expect(tap.message).toContain("cannot be answered by voice");
	});
});

describe("the cards never take a verdict that is not theirs", () => {
	test("allow, deny and comment on a choice card decide nothing", () => {
		for (const k of ["allow", "deny", "comment"]) expect(view(fx.approve[k], "choice").decided).toBeNull();
		expect(view(fx.approve.comment, "choice").note).toBeNull();
	});

	test("choice on an approval card decides nothing", () => {
		expect(view(fx.answer.choice, "approval").decided).toBeNull();
	});
});

describe("spoken text", () => {
	test("the crewmate row says it writes spoken lines", () => {
		expect(fx.crewmate.spoken_replies).toBe(true);
	});

	for (const key of ["task_agent_line", "task_first_sentences"]) {
		test(`${key}: a task that finishes in an open chat is read with exactly hostd's \`spoken\``, () => {
			const w = newWatch();
			nextSpoken(w, [{ ...fx[key], state: "running" }]);
			expect(nextSpoken(w, [fx[key]])).toBe(fx[key].spoken);
		});
	}

	test("a finished task with an empty `spoken` is not read", () => {
		const w = newWatch();
		nextSpoken(w, [{ ...fx.task_agent_line, state: "running" }]);
		expect(nextSpoken(w, [{ ...fx.task_agent_line, spoken: "" }])).toBeNull();
	});
});
