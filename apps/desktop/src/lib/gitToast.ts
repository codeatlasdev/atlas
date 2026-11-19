import { toast } from "@codeatlasdev/ui";
import { classifyGitError } from "#lib/gitErrors.ts";
import { m } from "#lib/i18n/m.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";

/** A command's own output as shown in the Details dialog: the end of a very long one is the part that has the cause. */
const DETAILS_MAX = 4000;

/** An error toast with one sentence from the catalog and the command's own output behind a Details button (#505). */
export function errorToast(message: string, details: string): void {
	if (!details) return void toast.error(message);
	toast.error(message, {
		action: {
			label: m.git_errors_details(),
			onClick: () =>
				void confirmDialog(message, {
					description: details.slice(0, DETAILS_MAX),
					confirmLabel: m.common_action_copy(),
				}).then((copy) => {
					if (copy) void navigator.clipboard.writeText(details);
				}),
		},
	});
}

/** An error toast for a failed git command: one sentence, and git's own output behind a Details button (#505). */
export function gitToast(raw: string): void {
	const p = classifyGitError(raw);
	errorToast(p.message, p.details);
}

/** An error toast for a failed `gh` command: `message` says what did not happen, gh's own words are the details. */
export function ghToast(raw: string, message = m.git_github_command_failed()): void {
	errorToast(message, raw.trim());
}
