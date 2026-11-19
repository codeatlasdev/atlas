/**
 * The words for values the Rust side sends about git: the merge or rebase in progress and the status of a file in a
 * diff. Each known value has a message; any other shows as it came. `mappedValues.test.ts` reads the lists the Rust
 * tests write (`__fixtures__/git-review-values.json`) and fails when a value has no message. Pure: no Svelte, no IPC.
 */
import { m } from "#lib/i18n/m.ts";

/** The merge or rebase in progress, as `git_review::operation` names it. */
export function operationText(op: string): string {
	switch (op) {
		case "merge":
			return m.git_commit_operation_merge();
		case "rebase":
			return m.git_commit_operation_rebase();
		default:
			return m.git_source_control_operation_in_progress({ operation: op });
	}
}

/** `FileDiff.status`, as `git_review::FILE_STATUSES` lists it. */
export function statusText(status: string): string {
	switch (status) {
		case "modified":
			return m.git_diff_view_status_modified();
		case "added":
			return m.git_diff_view_status_added();
		case "deleted":
			return m.git_diff_view_status_deleted();
		case "renamed":
			return m.git_diff_view_status_renamed();
		case "untracked":
			return m.git_diff_view_status_untracked();
		default:
			return status;
	}
}
