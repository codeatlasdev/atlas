// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../test/setup.ts";
import { expect, test } from "bun:test";
import { inLocale } from "../../test/locale.ts";
import { m } from "#lib/i18n/m.ts";
import commitErrors from "./__fixtures__/commit-msg-errors.json";
import reviewValues from "./__fixtures__/git-review-values.json";
import hygieneKinds from "./__fixtures__/hygiene-kinds.json";
import shellSources from "./__fixtures__/shell-sources.json";
import prChecks from "./__fixtures__/gh/pr-checks.json";
import prViewOpen from "./__fixtures__/gh/pr-view-open.json";
import { commitErrorText } from "./commitMessage.ts";
import { operationText, statusText } from "./gitText.ts";
import { checkStateText, prStateText, reviewDecisionText, reviewStateText } from "./prText.ts";
import { sourceText } from "./settings/shellSource.ts";
import { dirKindText, kindText } from "./settings/hygiene.ts";

// Values that come from another process (Rust, `gh`) and are shown inside the app's sentences. Each one has a message
// of the catalog, so a person in Portuguese never reads the raw value. The lists are the REAL output of the producers:
// the Rust tests write `__fixtures__/commit-msg-errors.json`, `git-review-values.json`, `hygiene-kinds.json` and
// `shell-sources.json`; `__fixtures__/gh` is recorded `gh` output. A value a producer adds fails here until it has a
// message. The language is pt-BR, where the raw value (English) and the message differ.

/** The message each producer value must show; keys are the values, so an unlisted one fails the "covers" tests. */
const EXPECT = {
	commit: {
		"commit_msg.ssh_host": () => m.git_commit_error_ssh(),
		"commit_msg.provider_off": () => m.git_commit_error_off(),
		"commit_msg.no_changes": () => m.git_commit_error_no_changes(),
		"commit_msg.all_secret": () => m.git_commit_error_all_secret(),
		"commit_msg.empty_message": () => m.git_commit_error_empty(),
	},
	operation: {
		merge: () => m.git_commit_operation_merge(),
		rebase: () => m.git_commit_operation_rebase(),
	},
	status: {
		modified: () => m.git_diff_view_status_modified(),
		added: () => m.git_diff_view_status_added(),
		deleted: () => m.git_diff_view_status_deleted(),
		renamed: () => m.git_diff_view_status_renamed(),
		untracked: () => m.git_diff_view_status_untracked(),
	},
	kind: {
		terminal: () => m.settings_general_hygiene_kind_terminal(),
		service: () => m.settings_general_hygiene_kind_service(),
		agent: () => m.settings_general_hygiene_kind_agent(),
		browser: () => m.settings_general_hygiene_kind_browser(),
		other: () => m.settings_general_hygiene_kind_other(),
	},
	dirKind: {
		temp: () => m.settings_general_hygiene_dir_temp(),
		browser_profile: () => m.settings_general_hygiene_dir_browser_profile(),
		cache: () => m.settings_general_hygiene_dir_cache(),
		instance: () => m.settings_general_hygiene_dir_instance(),
	},
	source: {
		settings: () => m.settings_terminal_shell_source_settings(),
		account: () => m.settings_terminal_shell_source_account(),
		$SHELL: () => m.settings_terminal_shell_source_env(),
		default: () => m.settings_terminal_shell_source_default(),
	},
};

function covers(name: string, produced: string[], table: Record<string, () => string>, show: (v: string) => string) {
	test(`${name}: every value the producer makes shows its message, in Portuguese too`, async () => {
		expect(produced.length).toBeGreaterThan(0);
		for (const v of produced) {
			expect(table[v], `${name}: "${v}" has no message in this test: add one to the catalog and to EXPECT`).toBeDefined();
			expect(show(v)).toBe(table[v]());
			await inLocale("pt-BR", () => {
				expect(show(v)).toBe(table[v]());
			});
		}
	});
	test(`${name}: a value nobody listed shows as it came`, () => {
		expect(show("brand_new_value")).toContain("brand_new_value");
	});
}

covers("commit errors", commitErrors, EXPECT.commit, commitErrorText);
covers("merge/rebase in progress", reviewValues.operations, EXPECT.operation, operationText);
covers("file status", reviewValues.statuses, EXPECT.status, statusText);
covers("hygiene process kind", hygieneKinds.kinds, EXPECT.kind, kindText);
covers("hygiene folder kind", hygieneKinds.dir_kinds, EXPECT.dirKind, dirKindText);
covers("shell source", shellSources, EXPECT.source, sourceText);

// `gh` output: the recorded real answers. Only the values GitHub's own enums list below are known beyond the recording.
test("recorded gh states show their messages, in Portuguese too", async () => {
	const checks = JSON.parse(prChecks.result.stdout);
	const view = JSON.parse(prViewOpen.result.stdout);
	await inLocale("pt-BR", () => {
		for (const c of checks) expect(checkStateText(c.state)).not.toBe(c.state.toLowerCase());
		expect(prStateText(view.state, view.isDraft)).toBe(m.git_github_pr_merged());
		expect(reviewDecisionText(view.reviewDecision)).toBe(m.git_github_review_required());
	});
});

test("every GitHub check, review and PR state this app words has its own message", async () => {
	const checks = ["SUCCESS", "FAILURE", "ERROR", "PENDING", "IN_PROGRESS", "QUEUED", "SKIPPED", "CANCELLED", "NEUTRAL", "TIMED_OUT"];
	const reviews = ["APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED", "PENDING"];
	await inLocale("pt-BR", () => {
		const shown = [...checks.map(checkStateText), ...reviews.map(reviewStateText)];
		for (const text of shown) expect(text).not.toMatch(/[A-Z_]{4}/);
		expect(new Set(checks.map(checkStateText)).size).toBe(checks.length);
		expect(prStateText("OPEN", true)).toBe(m.git_github_pr_draft());
		expect(["OPEN", "MERGED", "CLOSED"].map((s) => prStateText(s, false))).toEqual([
			m.git_github_pr_open(),
			m.git_github_pr_merged(),
			m.git_github_pr_closed(),
		]);
	});
	expect(checkStateText("SOMETHING_NEW")).toBe("something new");
});
