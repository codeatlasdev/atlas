import { toast } from "@codeatlasdev/ui";
import { git, errText } from "#lib/api.ts";
import { gitToast } from "#lib/gitToast.ts";
import { m } from "#lib/i18n/m.ts";
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
		gitToast(errText(e));
	}
}

/** Palette items for the typed query: matching branches, then "Create branch" for a new name. */
export function branchItems(q: string) {
	const data = branchPicker.data;
	if (!data) return [];
	return branchRows(data, q).map((r) => ({
		id: r.id,
		group: r.kind === "remote" ? m.git_branch_group_remote() : m.git_branch_group_local(),
		label: r.label,
		description: r.description,
		run: () => void pick(r),
	}));
}

export function createBranchItems(q: string) {
	const data = branchPicker.data;
	const r = data && createRow(data, q, data.current);
	return r
		? [
				{
					id: r.id,
					group: m.git_branch_group_create(),
					label: r.label,
					description: r.description,
					run: () => void pick(r),
				},
			]
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
		return void toast.error(m.git_branch_unknown_checkout({ branch: r.name, path: r.jump }));
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
					? m.git_branch_switched_stashed({ branch: out.branch })
					: opts.create
						? m.git_branch_created({ branch: out.branch })
						: m.git_branch_switched({ branch: out.branch }),
			);
		} else if (out.status === "worktree") {
			const target = worktreeAt(out.path);
			if (target) goWorktree(target.id);
			else toast.error(m.git_branch_checked_out_in({ branch, path: out.path }));
		} else if (
			await confirmDialog(m.git_branch_stash_title(), {
				// git's own words (which files are in the way), then the question.
				description: `${out.message}\n\n${m.git_branch_stash_question()}`,
				confirmLabel: m.git_branch_stash_confirm(),
			})
		) {
			await run(id, branch, { ...opts, stash: true });
		}
	} catch (e) {
		gitToast(errText(e));
	}
}
