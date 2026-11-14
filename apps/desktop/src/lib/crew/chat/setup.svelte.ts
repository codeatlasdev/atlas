/**
 * Owns: the state machine of the setup conversation a new crewmate walks through (name, role, project, approval,
 * summary): replaying answered steps, the typing delay, and mapping each answer onto the crewmate record.
 * Question text and step order live in crewSetup.ts; this file only drives them.
 */
import { toast } from "@codeatlasdev/ui";
import { invoke } from "@tauri-apps/api/core";
import { errText } from "#lib/api.ts";
import { crew } from "#lib/stores/crew.svelte.ts";
import type { Crewmate } from "#lib/crew.ts";
import { projectForServer } from "#lib/crew/projectRef.ts";
import {
	getQuestion,
	optionAnswer,
	nextStep as setupNextStep,
	roleKeyToText,
	approvalKeyToMode,
	summaryText,
	typingDelay,
	RESTART_STEP,
	STEPS,
	type SetupStep,
	type SetupQuestion,
	type SummaryState,
	type SummaryPart,
} from "#lib/crew/crewSetup.ts";

export interface SetupMessage {
	kind: "crewmate" | "user";
	text: string;
	parts?: SummaryPart[];
}

/** An IDE project the user can link in the project question. */
export interface IdeProject {
	id: string;
	name: string;
	path: string;
	branch: string;
}

interface Collected {
	name: string;
	roleKey: string;
	roleText: string;
	project: string;
	approvalKey: string;
}

export class SetupFlow {
	/** Messages already rendered (so resume shows them without typing delay). */
	messages = $state<SetupMessage[]>([]);
	/** Whether the crewmate is "typing" its next question. */
	typing = $state(false);
	/** The live question currently shown to the user (null while typing). */
	question = $state<SetupQuestion | null>(null);
	/** Free-text answer the user is composing. */
	input = $state("");
	/** Whether a crewmate_update call is in flight. */
	saving = $state(false);
	/** Accumulated answers; used to build the summary. */
	private collected = $state<Collected>({
		name: "",
		roleKey: "",
		roleText: "",
		project: "",
		approvalKey: "A",
	});

	/**
	 * (Re)initialise when a crewmate with a setup step is selected or the step changes. Replays answered steps
	 * without delay, then shows the current question after the typing indicator. Returns the timer cleanup.
	 */
	resume(step: SetupStep, b: Crewmate, ideProjectNames: string[]): () => void {
		// Pre-fill collected answers from the existing crewmate record so resuming works.
		const resumed: Collected = {
			name: b.name !== "New crewmate" ? b.name : "",
			roleKey: "",
			roleText: b.role ?? "",
			project: b.project?.name ?? b.project?.path ?? b.project?.url ?? "",
			approvalKey: b.approval === "auto" ? "C" : "A",
		};
		this.collected = resumed;

		// All steps BEFORE the current one are already answered.
		const currentIdx = STEPS.indexOf(step);
		const replay: SetupMessage[] = [];
		for (let i = 0; i < currentIdx; i++) {
			const s = STEPS[i];
			const q = getQuestion(s, { ideProjects: ideProjectNames });
			replay.push({ kind: "crewmate", text: q.text });
			let answer = "";
			if (s === "name") answer = resumed.name || b.name;
			// Only what was really saved: an empty role used to replay as the user saying "Something else".
			else if (s === "role") answer = resumed.roleText;
			else if (s === "project") answer = resumed.project || "Skip";
			else if (s === "approval")
				answer = getQuestion("approval").options?.find((o) => o.key === resumed.approvalKey)?.title ?? "";
			if (answer) replay.push({ kind: "user", text: answer });
		}
		this.messages = replay;
		this.question = null;

		// Now show typing indicator, then the current question.
		this.typing = true;
		const delay = replay.length === 0 ? 600 : typingDelay();
		const tid = setTimeout(() => {
			this.typing = false;
			if (step === "summary") {
				const parts = summaryText({
					name: resumed.name || b.name,
					roleText: resumed.roleText || "do tasks",
					project: resumed.project || undefined,
					approvalKey: resumed.approvalKey,
				} satisfies SummaryState);
				this.question = { ...getQuestion("summary"), parts, text: parts.map((p) => p.text).join("") };
			} else {
				this.question = getQuestion(step, { ideProjects: ideProjectNames });
			}
		}, delay);
		return () => clearTimeout(tid);
	}

	async answer(
		step: SetupStep | undefined,
		b: Crewmate | undefined,
		ideProjects: IdeProject[],
		key?: string,
		text?: string,
	): Promise<void> {
		if (!step || !b || !crew.selected || this.saving) return;

		const option = key != null ? this.question?.options?.find((o) => o.key === key) : undefined;
		// "Something else" asks for text: put the cursor in the box and wait, never save the option's own title.
		if (option && "write" in optionAnswer(option)) {
			document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Setup answer"]')?.focus();
			return;
		}
		const answerText = option ? option.title : (text ?? "").trim();
		if (!answerText && step !== "project") return;

		this.messages = [
			...this.messages,
			{ kind: "crewmate", text: this.question?.text ?? "", parts: this.question?.parts },
		];
		this.question = null;

		// Map answer to crewmate fields
		const patch: Record<string, unknown> = {};
		let next: SetupStep | "done" = "done";
		const newState = { ...this.collected };

		if (step === "name") {
			const name = answerText.trim() || "New crewmate";
			newState.name = name;
			patch.name = name;
			next = setupNextStep("name");
		} else if (step === "role") {
			if (!key) {
				// typed by the user
				newState.roleText = answerText;
			} else {
				newState.roleKey = key;
				newState.roleText = roleKeyToText(key);
			}
			patch.role = newState.roleText;
			next = setupNextStep("role");
		} else if (step === "project") {
			const projectName = answerText.trim();
			newState.project = projectName;
			if (projectName) {
				// One of the user's projects: resolve its git origin BEFORE saving (it used to save the Mac path and
				// race a later overwrite, so the crew server could keep a path it cannot open).
				const matched = ideProjects.find((p) => p.name === projectName);
				if (matched) {
					const origin = await invoke<{ url: string | null }>("project_git_info", { path: matched.path })
						.then((i) => i.url)
						.catch(() => null);
					const r = projectForServer(matched, origin, crew.selected.hostKey);
					if (!r.ok) {
						toast.error(r.error);
						return;
					}
					patch.project = r.project;
				} else {
					// Free text / git URL
					const isGit = /^(https?|git|ssh):\/\//.test(projectName) || projectName.endsWith(".git");
					patch.project = isGit
						? { kind: "git", url: projectName, name: projectName }
						: { kind: "path", path: projectName, name: projectName };
				}
			}
			next = setupNextStep("project");
		} else if (step === "approval") {
			newState.approvalKey = key ?? "A";
			patch.approval = approvalKeyToMode(key ?? "A");
			next = setupNextStep("approval");
		} else if (step === "summary") {
			if (key === "B") {
				// Restart: go back to first step
				this.messages = [];
				patch.setup_step = RESTART_STEP;
				next = RESTART_STEP;
			} else {
				// Done
				patch.setup_step = null;
				next = "done";
			}
		}

		this.collected = newState;
		if (step !== "summary" || key === "A") {
			this.messages = [...this.messages, { kind: "user", text: answerText }];
		}

		// Persist to crewmate record
		this.saving = true;
		try {
			type Patch = Parameters<typeof crew.updateCrewmate>[2];
			const updatePatch: Patch = {
				...("name" in patch ? { name: patch.name as string } : {}),
				...("role" in patch ? { role: patch.role as string } : {}),
				...("approval" in patch ? { approval: patch.approval as string } : {}),
				...("project" in patch ? { project: patch.project as Patch["project"] } : {}),
				...(step !== "summary" || key !== "B"
					? { setup_step: next === "done" ? null : (next as string) }
					: { setup_step: RESTART_STEP }),
			};
			await crew.updateCrewmate(crew.selected.hostKey, b.id, updatePatch);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			this.saving = false;
		}

		this.input = "";

		// If done, the resume effect won't re-run (setup_step cleared), so just clear state.
		if (next === "done") {
			this.messages = [];
			this.question = null;
		}
	}
}
