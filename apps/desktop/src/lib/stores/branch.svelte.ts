import { toast } from "@codeatlasdev/ui";
import { git, errText } from "#lib/api.ts";
import { branchRows, createRow, type BranchRow, type Branches } from "#lib/branches.ts";
import { goWorktree } from "#lib/shell/navigate.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";

/** The branch picker: open flag, the branches of the worktree it was opened for, and the actions it runs. */
export const branchPicker = $state<{ open: boolean; worktreeId: string; data: Branches | null }>({
	open: false,
	worktreeId: "",
	data: null,
});

/** Open the picker for the selected worktree (the list loads before it shows, so the first frame is not empty). */
export async function openBranchPicker() {
	const w = ws.selected;
	if (!w) return;
	try {
		branchPicker.data = await git.branches(w.id);
		branchPicker.worktreeId = w.id;
		branchPicker.open = true;
	} catch (e) {
		toast.error(errText(e));
	}
}

/** Palette items for the typed query: matching branches, then "Create branch" for a new name. */
export function branchItems(q: string) {
	const data = branchPicker.data;
	if (!data) return [];
	return branchRows(data, q).map((r) => ({
		id: r.id,
		group: r.kind === "remote" ? "Remote branches" : "Branches",
		label: r.label,
		description: r.description,
		run: () => void pick(r),
	}));
}

export function createBranchItems(q: string) {
	const data = branchPicker.data;
	const r = data && createRow(data, q, data.current);
	return r
		? [{ id: r.id, group: "Create", label: r.label, description: r.description, run: () => void pick(r) }]
		: [];
}

/** The worktree of this repo whose checkout lives at `path`. */
function worktreeAt(path: string) {
	return store.projects.find((p) => p.root === path);
}

async function pick(r: BranchRow) {
	const id = branchPicker.worktreeId;
	// Checked out elsewhere: git refuses a second checkout, so go to that worktree instead.
	if (r.jump) {
		const target = worktreeAt(r.jump);
		if (target) return goWorktree(target.id);
		return void toast.error(`${r.name} is checked out in ${r.jump}, which Atlas does not know`);
	}
	if (r.current) return;
	await run(id, r.name, { create: r.kind === "create" });
}

async function run(id: string, branch: string, opts: { create?: boolean; stash?: boolean }) {
	try {
		const out = await git.switch(id, branch, opts);
		if (out.status === "ok") {
			toast.success(
				out.stashed
					? `Switched to ${out.branch}; your changes are in the stash`
					: opts.create
						? `Created and switched to ${out.branch}`
						: `Switched to ${out.branch}`,
			);
		} else if (out.status === "worktree") {
			const target = worktreeAt(out.path);
			if (target) goWorktree(target.id);
			else toast.error(`${branch} is checked out in ${out.path}`);
		} else if (
			await confirmDialog(`Local changes would be overwritten`, {
				description: `${out.message}\n\nStash them (untracked files too) and switch? They stay in \`git stash\`.`,
				confirmLabel: "Stash and switch",
			})
		) {
			await run(id, branch, { ...opts, stash: true });
		}
	} catch (e) {
		toast.error(errText(e));
	}
}
