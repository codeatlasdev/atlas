// @ts-nocheck bun test runtime types are not part of the app tsconfig
// Contract (#201): the Audit section reads the real `crewmate_audit` reply. The fixture is written by the Rust
// serializer (UPDATE_FIXTURES=1 cargo test -p atlas-host crewmate_audit_fixture_is_real_payload), never by hand:
// a hand-written mock hid a snake_case field that crashed every real build (languageLabels, 1.3.0).
import { describe, expect, test } from "bun:test";
import fixture from "../../../__fixtures__/crewmate_audit.json" with { type: "json" };
import {
	AUDIT_KINDS,
	auditCopyText,
	auditKindLabel,
	auditSummary,
	filterAudit,
	parseAudit,
	TASK_STATE_MESSAGES,
	taskStateLabel,
} from "./audit.ts";
import { rustEnumValues } from "../../panel/stateLabel.test.ts";

describe("parseAudit: the real crewmate_audit reply", () => {
	test("reads every entry of the reply, newest first as sent", () => {
		const entries = parseAudit(fixture);
		expect(entries.map((e) => e.seq)).toEqual(fixture.entries.map((e) => e.seq));
		expect(entries[0].seq).toBeGreaterThan(entries[entries.length - 1].seq);
	});

	test("every kind hostd wrote is one the section knows (a new kind must be added to the filter)", () => {
		const known = new Set(AUDIT_KINDS.map((k) => k.kind));
		for (const e of parseAudit(fixture)) expect(known.has(e.kind)).toBe(true);
	});

	test("an older server without the command, or garbage, reads as no entries", () => {
		expect(parseAudit(undefined)).toEqual([]);
		expect(parseAudit({})).toEqual([]);
		expect(parseAudit({ entries: "x" })).toEqual([]);
		expect(parseAudit({ entries: [null, 3, { kind: "tool_call" }] })).toEqual([]); // no ts/seq: not an entry
	});
});

describe("auditSummary: one line per entry, from the keys hostd really sends", () => {
	const by = (kind) => parseAudit(fixture).find((e) => e.kind === kind);

	test("a tool call names the tool and its input", () => {
		expect(auditSummary(by("tool_call"))).toBe("Bash: cargo test -p atlas-host");
	});

	test("an approval decision says allow/deny, the tool and who answered", () => {
		expect(auditSummary(by("approval_decision"))).toBe("Allowed Bash (answered by desktop)");
	});

	test("an approval nobody answered says so", () => {
		const e = { ...by("approval_decision"), decision: "timeout", by: "none" };
		expect(auditSummary(e)).toBe("Timed out Bash (nobody answered)");
	});

	test("phone and telegram answerers are named readably", () => {
		const e = { ...by("approval_decision"), decision: "deny", by: "phone:abcdef1234567890" };
		expect(auditSummary(e)).toBe("Denied Bash (answered by phone abcdef12…)");
		expect(auditSummary({ ...e, by: "telegram:42" })).toBe("Denied Bash (answered by Telegram chat 42)");
	});

	test("a config change shows field, before and after", () => {
		const e = by("config_changed");
		expect(auditSummary(e)).toBe(`Changed ${e.field}: ${JSON.stringify(e.from)} → ${JSON.stringify(e.to)}`);
	});

	test("a secret entry names the secret and never has a value to show", () => {
		expect(auditSummary(by("secret_set"))).toBe("Set secret GITHUB_TOKEN");
	});

	test("task lifecycle lines carry the state", () => {
		expect(auditSummary(by("task_finished"))).toBe("Task finished: done");
		expect(auditSummary(by("task_started"))).toContain("Task started");
		expect(auditSummary(by("task_queued"))).toContain("run the tests");
	});

	test("a kind from a newer server still renders (its name), never throws", () => {
		expect(auditSummary({ seq: 1, ts: 1, kind: "something_new" })).toBe("something_new");
	});

	test("a long tool input is shortened for the list (the full text stays in copy)", () => {
		const e = {
			seq: 1,
			ts: 1,
			kind: "tool_call",
			tool_name: "Write",
			tool_input: { content: "x".repeat(500) },
		};
		expect(auditSummary(e).length).toBeLessThan(160);
		expect(auditCopyText(e)).toContain("x".repeat(500));
	});
});

describe("filterAudit", () => {
	const entries = parseAudit(fixture);

	test("no kind keeps everything", () => {
		expect(filterAudit(entries, null)).toHaveLength(entries.length);
	});

	test("a group matches its kinds only", () => {
		const approvals = filterAudit(entries, "approvals");
		expect(approvals.length).toBeGreaterThan(0);
		expect(approvals.every((e) => e.kind.startsWith("approval_"))).toBe(true);
		const tools = filterAudit(entries, "tools");
		expect(tools.every((e) => e.kind === "tool_call")).toBe(true);
	});

	test("every kind belongs to exactly one group", () => {
		for (const { kind } of AUDIT_KINDS) {
			const groups = ["tools", "approvals", "tasks", "config"].filter(
				(g) => filterAudit([{ seq: 1, ts: 1, kind }], g).length,
			);
			expect(groups).toHaveLength(1);
		}
	});
});

describe("auditCopyText / auditKindLabel", () => {
	test("copy is the entry as pretty JSON with its time readable first", () => {
		const e = parseAudit(fixture)[0];
		const text = auditCopyText(e);
		expect(JSON.parse(text.slice(text.indexOf("{")))).toEqual(e);
	});

	test("labels are human", () => {
		expect(auditKindLabel("approval_decision")).toBe("Approval decided");
		expect(auditKindLabel("zzz")).toBe("zzz");
	});
});

describe("task states: the values hostd records", () => {
	test("every TaskState variant has a message, and the fixture's states are among them", () => {
		const sent = rustEnumValues("crates/atlas-host/src/crew/store/mod.rs", "TaskState");
		expect(sent).toContain("waiting_action");
		for (const s of sent)
			expect(Object.hasOwn(TASK_STATE_MESSAGES, s), `no message for task state ${s}`).toBe(true);
		for (const e of fixture.entries) if (e.kind === "task_finished") expect(sent).toContain(e.state);
	});

	test("a state this build does not know shows as sent", () => {
		expect(taskStateLabel("exploded")).toBe("exploded");
		expect(auditSummary({ seq: 1, ts: 1, kind: "task_finished", state: "exploded" })).toBe(
			"Task finished: exploded",
		);
	});
});
