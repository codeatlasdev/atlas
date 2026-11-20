// @ts-nocheck
/**
 * Tests for the crewSetup state machine.
 * Guards step order, option mapping, summary text, and resume logic.
 */
import { describe, expect, it } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";
import {
	CREW_NAMES,
	STEPS,
	getQuestion,
	normalizeStep,
	optionAnswer,
	nextStep,
	RESTART_STEP,
	roleKeyToText,
	summaryText,
	typingDelay,
	type SetupStep,
} from "./crewSetup.ts";

// ─── step ordering ────────────────────────────────────────────────────────────

describe("step ordering", () => {
	it("STEPS has the four expected steps in order", () => {
		expect(STEPS).toEqual(["name", "role", "project", "summary"]);
	});

	it("nextStep advances through all steps then returns done", () => {
		const visited: string[] = [];
		let step: SetupStep | "done" = "name";
		while (step !== "done") {
			visited.push(step);
			step = nextStep(step as SetupStep);
		}
		expect(visited).toEqual(STEPS);
	});

	it("nextStep from summary returns done", () => {
		expect(nextStep("summary")).toBe("done");
	});

	it("RESTART_STEP is name", () => {
		expect(RESTART_STEP).toBe("name");
	});
});

// ─── name step ────────────────────────────────────────────────────────────────

describe("name step", () => {
	it("uses fallback CREW_NAMES when no IDE projects supplied", () => {
		const q = getQuestion("name");
		expect(q.nameChips).toEqual([...CREW_NAMES]);
		expect(q.allowText).toBe(true);
		expect(q.options).toBeUndefined();
	});

	it("uses first 3 IDE project names as chips when available", () => {
		const q = getQuestion("name", { ideProjects: ["atlas", "portal", "infra", "legacy"] });
		expect(q.nameChips).toEqual(["atlas", "portal", "infra"]);
	});

	it("question text is set", () => {
		expect(getQuestion("name").text).toBeTruthy();
	});
});

// ─── role step ────────────────────────────────────────────────────────────────

describe("role step", () => {
	it("has four lettered options A-D", () => {
		const q = getQuestion("role");
		const keys = q.options?.map((o) => o.key);
		expect(keys).toEqual(["A", "B", "C", "D"]);
	});

	it("allows free text for D (something else)", () => {
		expect(getQuestion("role").allowText).toBe(true);
	});

	it("roleKeyToText maps A/B/C to role strings", () => {
		expect(roleKeyToText("A")).toBe("Review every PR");
		expect(roleKeyToText("B")).toBe("Fix failing CI");
		expect(roleKeyToText("C")).toBe("Write the changelog");
	});

	it("roleKeyToText returns empty string for D (free text)", () => {
		expect(roleKeyToText("D")).toBe("");
	});
});

// ─── project step ─────────────────────────────────────────────────────────────

describe("project step", () => {
	it("falls back to free text only when no IDE projects", () => {
		const q = getQuestion("project");
		expect(q.options).toBeUndefined();
		expect(q.allowText).toBe(true);
	});

	it("shows IDE projects as options with a trailing 'Something else'", () => {
		const q = getQuestion("project", { ideProjects: ["web", "api"] });
		expect(q.options?.length).toBe(3); // 2 projects + 1 "Something else"
		expect(q.options?.[0].title).toBe("web");
		expect(q.options?.[1].title).toBe("api");
		expect(q.options?.[2].title).toBe("Something else");
	});

	it("caps project options at 3 + trailing 'Something else'", () => {
		const q = getQuestion("project", { ideProjects: ["a", "b", "c", "d"] });
		expect(q.options?.length).toBe(4); // 3 capped + 1 "Something else"
	});
});

// Incident: picking "Something else" (meant to let you type) was taken as the answer itself: a new crewmate got the
// project folder "Something else" ("project prepare: path error: No such file or directory") and carlos the role
// "Something else". Such an option asks for text; only a real option or typed text answers.
describe("options that ask you to type", () => {
	it("'Something else' on the project and role questions waits for text instead of answering", () => {
		const project = getQuestion("project", { ideProjects: ["web"] });
		const role = getQuestion("role");
		expect(optionAnswer(project.options.at(-1))).toEqual({ write: true });
		expect(optionAnswer(role.options.at(-1))).toEqual({ write: true });
	});

	it("a real option answers with its title", () => {
		const project = getQuestion("project", { ideProjects: ["web"] });
		expect(optionAnswer(project.options[0])).toEqual({ answer: "web" });
		expect(optionAnswer(getQuestion("role").options[0])).toEqual({ answer: "Review every PR" });
	});
});

// ─── no approval step ─────────────────────────────────────────────────────────

// #642: the mode a crewmate starts in is the crew's default, set in Settings and switched from the message box; the
// setup conversation no longer asks "how careful should I be" (it forced Ask or Bypass, ignoring the crew default).
describe("no approval step", () => {
	it("a crewmate saved at the old approval step resumes at the summary", () => {
		expect(normalizeStep("approval")).toBe("summary");
	});

	it("the other persisted steps read as themselves; an unknown or missing one is no step", () => {
		for (const s of STEPS) expect(normalizeStep(s)).toBe(s);
		expect(normalizeStep("bogus")).toBeUndefined();
		expect(normalizeStep(undefined)).toBeUndefined();
	});
});

// ─── summary step ─────────────────────────────────────────────────────────────

describe("summary step", () => {
	it("has exactly two options: Looks good and Change something", () => {
		const q = getQuestion("summary");
		expect(q.options?.length).toBe(2);
		expect(q.options?.[0].key).toBe("A");
		expect(q.options?.[1].key).toBe("B");
	});

	it("does not allow free text", () => {
		expect(getQuestion("summary").allowText).toBe(false);
	});

	// Incident: the summary bubble showed "I'm **Jorge**. I'll review every pr on **https://github.com/…/promptin**":
	// the markdown stars were printed, "PR" lost its case and the project was a full URL. The summary is now parts the
	// chat renders (bold where it matters), never markdown.
	const plain = (parts) => parts.map((p) => p.text).join("");
	const bold = (parts) => parts.filter((p) => p.bold).map((p) => p.text);

	it("names the crewmate, keeps the role's own case and says the mode it starts in, without markdown", () => {
		const parts = summaryText({ name: "Jorge", roleText: "Review every PR", mode: "ask" });
		expect(plain(parts)).toBe("I'm Jorge. I'll review every PR, and I'll start in Ask mode.");
		expect(bold(parts)).toEqual(["Jorge", "Ask"]);
		expect(plain(parts)).not.toContain("*");
	});

	it("shows a GitHub project as owner/repo, in bold", () => {
		const parts = summaryText({
			name: "Jorge",
			roleText: "Review every PR",
			project: "https://github.com/codeatlasdev/promptin.git",
			mode: "edits",
		});
		expect(plain(parts)).toContain(
			"review every PR on codeatlasdev/promptin, and I'll start in Accept edits mode",
		);
		expect(bold(parts)).toEqual(["Jorge", "codeatlasdev/promptin", "Accept edits"]);
	});

	it("keeps a project name or path as given, and omits the clause without one", () => {
		expect(
			bold(summaryText({ name: "Z", roleText: "Fix failing CI", project: "atlas", mode: "bypass" })),
		).toEqual(["Z", "atlas", "Bypass"]);
		expect(plain(summaryText({ name: "N", roleText: "Write the changelog", mode: "ask" }))).not.toContain(
			" on ",
		);
	});
});

// ─── restart from summary ─────────────────────────────────────────────────────

describe("restart from summary (B answer)", () => {
	it("RESTART_STEP is name so restarting goes back to the beginning", () => {
		// Simulate: user is on summary, picks B → restart at RESTART_STEP
		expect(RESTART_STEP).toBe("name");
		// After restart, nextStep from name follows normal order
		expect(nextStep(RESTART_STEP)).toBe("role");
	});
});

// ─── resume from persisted step ───────────────────────────────────────────────

describe("resume from persisted step", () => {
	it("each step name in STEPS is a valid SetupStep string", () => {
		for (const s of STEPS) {
			// getQuestion must not throw for any persisted step value
			expect(() => getQuestion(s)).not.toThrow();
		}
	});

	it("resuming mid-flow: nextStep from role reaches summary in 2 advances", () => {
		let step: SetupStep | "done" = "role";
		let advances = 0;
		while (step !== "done" && step !== "summary") {
			step = nextStep(step as SetupStep);
			advances++;
		}
		expect(advances).toBe(2); // role→project→summary
		expect(step).toBe("summary");
	});
});

// ─── typing delay ─────────────────────────────────────────────────────────────

describe("typingDelay", () => {
	it("returns a value in [800, 1200] ms", () => {
		for (let i = 0; i < 20; i++) {
			const d = typingDelay(Math.random());
			expect(d).toBeGreaterThanOrEqual(800);
			expect(d).toBeLessThanOrEqual(1200);
		}
	});

	it("is deterministic given the same seed", () => {
		expect(typingDelay(0.5)).toBe(typingDelay(0.5));
	});
});

// The summary is one catalog sentence per case: word order belongs to the language, and the bold runs are still the
// name and the project wherever the sentence puts them.
describe("summary in another language", () => {
	it("follows the catalog sentence and still bolds the name and the project", async () => {
		await inLocale("pt-BR", () => {
			const role = roleKeyToText("A");
			const parts = summaryText({ name: "Jorge", roleText: role, project: "atlas", mode: "ask" });
			expect(parts.filter((p) => p.bold).map((p) => p.text)).toEqual(["Jorge", "atlas", "Perguntar"]);
			const plain = parts.map((p) => p.text).join("");
			expect(plain).not.toContain("\u2063");
			expect(plain).toBe(
				m.crew_workspace_setup_summary_project({
					name: "Jorge",
					role: role.charAt(0).toLowerCase() + role.slice(1),
					project: "atlas",
					mode: "Perguntar",
				}),
			);
		});
	});

	it("builds the questions and options in the language of the moment", async () => {
		const english = getQuestion("role").options.map((o) => o.title);
		const other = await inLocale("pt-BR", () => getQuestion("role").options.map((o) => o.title));
		expect(other).not.toEqual(english);
		expect(other).toEqual(
			await inLocale("pt-BR", () => [
				m.crew_workspace_setup_role_review_title(),
				m.crew_workspace_setup_role_ci_title(),
				m.crew_workspace_setup_role_changelog_title(),
				m.crew_workspace_setup_something_else(),
			]),
		);
	});
});
