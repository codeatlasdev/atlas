/**
 * The words for what `gh` reports about a pull request (`gh pr view --json`, `gh pr checks --json`): states, review
 * decisions, check states. `gh` sends GitHub's own upper-case enum values; each known one has a message, and a value
 * this list does not know shows lower-cased as it came (a new GitHub state is readable, never blank).
 * `prText.test.ts` reads the recorded real `gh` output in `__fixtures__/gh` and fails when a value in it has no message.
 * Pure: no Svelte, no IPC.
 */
import { m } from "#lib/i18n/m.ts";
import type { rollup } from "#lib/stores/pr.svelte.ts";

type Rollup = ReturnType<typeof rollup>;

/** `OPEN`, `MERGED`, `CLOSED`, or a draft. */
export function prStateText(state: string, isDraft: boolean): string {
	if (isDraft) return m.git_github_pr_draft();
	switch (state) {
		case "OPEN":
			return m.git_github_pr_open();
		case "MERGED":
			return m.git_github_pr_merged();
		case "CLOSED":
			return m.git_github_pr_closed();
		default:
			return state.toLowerCase();
	}
}

/** `reviewDecision` of the pull request. */
export function reviewDecisionText(decision: string): string {
	switch (decision) {
		case "APPROVED":
			return m.git_github_review_approved();
		case "CHANGES_REQUESTED":
			return m.git_github_review_changes();
		case "REVIEW_REQUIRED":
			return m.git_github_review_required();
		default:
			return decision.toLowerCase();
	}
}

/** `state` of one review in the timeline (`APPROVED` and `CHANGES_REQUESTED` read as the decisions do). */
export function reviewStateText(state: string): string {
	switch (state) {
		case "COMMENTED":
			return m.git_github_review_commented();
		case "DISMISSED":
			return m.git_github_review_dismissed();
		case "PENDING":
			return m.git_github_check_pending();
		default:
			return reviewDecisionText(state);
	}
}

/** `state` of one check of `gh pr checks`. */
export function checkStateText(state: string): string {
	switch (state) {
		case "SUCCESS":
			return m.git_github_check_success();
		case "FAILURE":
			return m.git_github_check_failure();
		case "ERROR":
			return m.git_github_check_error();
		case "PENDING":
			return m.git_github_check_pending();
		case "IN_PROGRESS":
			return m.git_github_check_in_progress();
		case "QUEUED":
			return m.git_github_check_queued();
		case "SKIPPED":
			return m.git_github_check_skipped();
		case "CANCELLED":
			return m.git_github_check_cancelled();
		case "NEUTRAL":
			return m.git_github_check_neutral();
		case "TIMED_OUT":
			return m.git_github_check_timed_out();
		default:
			return state.toLowerCase().replaceAll("_", " ");
	}
}

/** The chip next to the title: how the checks of the pull request add up. */
export function checksChipText(rollup: Rollup): string {
	switch (rollup) {
		case "failed":
			return m.git_github_checks_chip_failed();
		case "pending":
			return m.git_github_checks_chip_pending();
		case "ok":
			return m.git_github_checks_chip_ok();
		case "none":
			return "";
	}
}

/** The word next to the "Checks" heading. */
export function rollupText(rollup: Rollup): string {
	switch (rollup) {
		case "failed":
			return m.git_github_rollup_failed();
		case "pending":
			return m.git_github_rollup_pending();
		case "ok":
			return m.git_github_rollup_ok();
		case "none":
			return "";
	}
}
