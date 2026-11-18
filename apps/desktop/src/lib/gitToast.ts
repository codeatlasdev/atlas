import { toast } from "@codeatlasdev/ui";
import { classifyGitError } from "#lib/gitErrors.ts";
import { confirmDialog } from "#lib/stores/ask.svelte.ts";
import { m } from "#lib/i18n/m.ts";

/** Git's output as shown in the Details dialog: the end of a very long one is the part that has the cause. */
const DETAILS_MAX = 4000;

/** An error toast for a failed git command: one sentence, and git's own output behind a Details button (#505). */
export function gitToast(raw: string): void {
	const p = classifyGitError(raw || "git failed");
	if (!p.details) return void toast.error(p.message);
	toast.error(p.message, {
		action: {
			label: m.errors_panel_details(),
			onClick: () =>
				void confirmDialog(p.message, {
					description: p.details.slice(0, DETAILS_MAX),
					confirmLabel: m.common_action_copy(),
				}).then((copy) => {
					if (copy) void navigator.clipboard.writeText(p.details);
				}),
		},
	});
}
