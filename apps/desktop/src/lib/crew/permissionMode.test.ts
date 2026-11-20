// @ts-nocheck bun test runtime types are not part of the app tsconfig
// Contract (#642): the desktop reads permission modes from the payloads hostd's own serializer wrote
// (UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::permissions), never from a hand-written mock.
import { describe, expect, test } from "bun:test";
import fx from "../../__fixtures__/crew_permission_modes.json" with { type: "json" };
import { render } from "../hostMessages.ts";
import { inLocale } from "../../../test/locale.ts";
import {
	COMMENT_MAX,
	DEFAULT_POLICY,
	approveArgs,
	canGrantForTask,
	makeAnswer,
	modeIsAmber,
	modeLabel,
	modeLocked,
	modeOf,
	parsePolicy,
	railText,
	selectableModes,
} from "./permissionMode.ts";
import { normalizeCrewmate } from "./wire.ts";

describe("modeOf: the mode of a real crewmate", () => {
	test("reads hostd's `mode`", () => {
		expect(modeOf(fx.crewmate_bypass)).toBe("bypass");
		expect(modeOf(fx.crewmate_edits)).toBe("edits");
		expect(modeOf(fx.set_approval)).toBe("edits");
	});

	test("survives the store's normalisation", () => {
		expect(modeOf(normalizeCrewmate(fx.crewmate_edits))).toBe("edits");
	});

	test("a crew server that only sends the old `approval` reads auto as bypass, ask as ask", () => {
		const { mode: _a, ...old } = fx.crewmate_bypass;
		const { mode: _b, ...oldAsk } = fx.crewmate_edits;
		expect(modeOf(old)).toBe("bypass");
		expect(modeOf(oldAsk)).toBe("ask");
	});

	test("`approval` is `auto` exactly when the mode is bypass (the contract the fallback leans on)", () => {
		for (const c of [fx.crewmate_bypass, fx.crewmate_edits, fx.crewmate_unsupported, fx.set_approval])
			expect(c.approval === "auto").toBe(c.mode === "bypass");
	});
});

describe("an engine that cannot ask", () => {
	test("the real codex crewmate is fixed in bypass", () => {
		expect(modeLocked(fx.crewmate_unsupported)).toBe(true);
		expect(modeOf(fx.crewmate_unsupported)).toBe("bypass");
	});

	test("an engine that can ask, and a terminal crewmate with no engine, can be switched", () => {
		expect(modeLocked(fx.crewmate_edits)).toBe(false);
		const { approval_supported: _, ...noEngine } = fx.crewmate_edits;
		expect(modeLocked(noEngine)).toBe(false);
	});
});

describe("crew policy", () => {
	test("reads the real crew_settings reply", () => {
		expect(parsePolicy(fx.settings)).toEqual({ default_approval: "ask", allow_bypass: true });
		expect(parsePolicy(fx.settings_no_bypass)).toEqual({ default_approval: "ask", allow_bypass: false });
	});

	test("a hostd from before permission modes reads as the defaults; junk never becomes a mode", () => {
		expect(parsePolicy({})).toEqual(DEFAULT_POLICY);
		expect(parsePolicy({ default_approval: "auto", allow_bypass: "yes" })).toEqual(DEFAULT_POLICY);
	});

	test("Bypass is offered only while the crew allows it", () => {
		expect(selectableModes(parsePolicy(fx.settings))).toEqual(["ask", "edits", "bypass"]);
		expect(selectableModes(parsePolicy(fx.settings_no_bypass))).toEqual(["ask", "edits"]);
	});
});

describe("colour", () => {
	test("Bypass is the only amber mode", () => {
		expect(["ask", "edits", "bypass"].filter(modeIsAmber)).toEqual(["bypass"]);
	});

	test("every mode has a label in both languages", async () => {
		expect(["ask", "edits", "bypass"].map(modeLabel)).toEqual(["Ask", "Accept edits", "Bypass"]);
		expect(await inLocale("pt-BR", () => ["ask", "edits"].map(modeLabel))).toEqual([
			"Perguntar",
			"Aceitar edições",
		]);
	});
});

describe("a prompt held by a rail", () => {
	test("the real pending approval names its rail and offers no task grant", () => {
		expect(fx.pending_approval.rail).toBe("protected_path");
		expect(canGrantForTask(fx.pending_approval)).toBe(false);
		expect(railText(fx.pending_approval.rail)).toContain("protected");
		expect(railText("no_auto")).toContain("GitHub");
	});

	test("an ordinary prompt can be allowed for the task", () => {
		const { rail: _, ...plain } = fx.pending_approval;
		expect(canGrantForTask(plain)).toBe(true);
	});
});

describe("answers", () => {
	test("Allow once is the default and carries no comment when the note is blank", () => {
		const a = makeAnswer(true, "once", "   ");
		expect(a).toEqual({ allow: true, scope: "once" });
		expect(approveArgs("p", "r", a)).toEqual({ pane: "p", req: "r", allow: true, scope: "once" });
	});

	test("Allow for this task keeps the scope and the trimmed note", () => {
		expect(approveArgs("p", "r", makeAnswer(true, "task", "  only in src  "))).toEqual({
			pane: "p",
			req: "r",
			allow: true,
			scope: "task",
			comment: "only in src",
		});
	});

	test("a deny never asks for a task grant, and its note reaches the agent", () => {
		expect(makeAnswer(false, "task", "use a branch")).toEqual({
			allow: false,
			scope: "once",
			comment: "use a branch",
		});
	});

	test("a note longer than hostd takes is cut, not refused", () => {
		expect(makeAnswer(false, "once", "x".repeat(COMMENT_MAX + 50)).comment).toHaveLength(COMMENT_MAX);
	});
});

describe("coded refusals", () => {
	test("the real refusal reads in the catalog (not the English message) in pt-BR", async () => {
		expect(render(fx.refusal.code, fx.refusal.args)).toBeTruthy();
		const pt = await inLocale("pt-BR", () => render(fx.refusal.code, fx.refusal.args));
		expect(pt).not.toBe(fx.refusal.message);
		expect(pt).toBeTruthy();
	});
});
