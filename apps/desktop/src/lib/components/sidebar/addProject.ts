import type { Probe, ProjectView } from "#lib/api.ts";
import { m } from "#lib/i18n/m.ts";
import { STACK_MESSAGES } from "#lib/onboarding/flow.ts";

/** What the Add flow does with a folder, decided from `project_probe` alone. Nothing is ever written to the folder:
 * a folder without `atlas.yaml` is added with its stack detected in memory (the project header offers to save it). */
export type AddPlan =
	/** Already in the sidebar: just select it. */
	| { kind: "select"; id: string }
	/** Register `root` (where the project lives). `inside` = the folder picked is a subfolder of it. */
	| { kind: "add"; root: string; inside: boolean };

export function addPlan(p: Probe): AddPlan {
	if (p.registered) return { kind: "select", id: p.registered };
	return { kind: "add", root: p.root, inside: p.root !== p.picked };
}

/**
 * The stack named in the "Detected ..." line. `detected` is the label `detection_label` (atlas-core `domain/project.rs`)
 * builds, `Node project (package.json)`: its kind is a stack id the app words, and a kind it does not know shows as
 * the label came.
 */
function detectedStack(detected: string): string {
	const kind = /^(\w+) project\b/.exec(detected)?.[1]?.toLowerCase();
	return kind && Object.hasOwn(STACK_MESSAGES, kind) ? STACK_MESSAGES[kind]!() : detected.replace(/ \(.*\)$/, "");
}

/** The line shown after adding a folder, or null when there is nothing to tell (it has an atlas.yaml, or is broken).
 * `detected` is the label from `project_config` (`Node project (package.json)`). */
export function addedToast(
	p: Pick<ProjectView, "config_source" | "services_total" | "error">,
	detected: string | null,
) {
	if (p.config_source !== "detected" || p.error) return null;
	if (!detected) return m.sidebar_added_nothing();
	return m.sidebar_added_detected({ stack: detectedStack(detected), count: p.services_total });
}

/** What removing a project does, said before it happens: the folder stays; a running stack stops; worktrees go too. */
export function removeDescription(p: Pick<ProjectView, "root" | "running">, linkedWorktrees: number): string {
	const lines = [p.root, m.sidebar_remove_note()];
	if (p.running) lines.push(m.sidebar_remove_stops());
	if (linkedWorktrees) lines.push(m.sidebar_remove_linked({ count: linkedWorktrees }));
	return lines.join("\n");
}

/** A Tauri drag-drop position is in physical pixels; the sidebar's rect is in CSS pixels. */
export function droppedOn(
	rect: { left: number; right: number; top: number; bottom: number },
	pos: { x: number; y: number },
	dpr: number,
): boolean {
	const x = pos.x / dpr;
	const y = pos.y / dpr;
	return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}
