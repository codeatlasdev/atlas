import type { InitPreview, Probe, ProjectView } from "#lib/api.ts";

/** What the Add flow does with a folder, decided from `project_probe` alone. */
export type AddPlan =
	/** Already in the sidebar: just select it. */
	| { kind: "select"; id: string }
	/** `atlas.yaml` at (or above) the folder: register `root`. `inside` = the folder picked is a subfolder of it. */
	| { kind: "add"; root: string; inside: boolean }
	/** No `atlas.yaml`: offer to create one with `atlas init`, then register. */
	| { kind: "init"; dir: string };

export function addPlan(p: Probe): AddPlan {
	if (p.registered) return { kind: "select", id: p.registered };
	if (p.has_config) return { kind: "add", root: p.root, inside: p.root !== p.picked };
	return { kind: "init", dir: p.picked };
}

/** Text of the confirmation shown before a config is created: what was detected, then the file itself. */
export function initDescription(dir: string, pv: InitPreview): string {
	const found = pv.detected
		? `Detected ${pv.detected}${pv.services.length ? `: ${pv.services.join(", ")}` : ""}.`
		: "Nothing recognised: the file has commented examples to edit.";
	return `${dir}\n${found}\nThis creates atlas.yaml in the folder:\n\n${pv.yaml.trimEnd()}`;
}

/** What removing a project does, said before it happens: the folder stays; a running stack stops; worktrees go too. */
export function removeDescription(p: Pick<ProjectView, "root" | "running">, linkedWorktrees: number): string {
	const lines = [p.root, "Only the Atlas entry is removed: no file or folder on disk is touched."];
	if (p.running) lines.push("Its running stack is stopped first.");
	if (linkedWorktrees) {
		lines.push(
			`Its ${linkedWorktrees} linked worktree${linkedWorktrees > 1 ? "s are" : " is"} removed from Atlas too.`,
		);
	}
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
