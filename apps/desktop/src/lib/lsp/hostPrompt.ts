/** Copy of the confirmation before restarting the terminal host (it ends running terminals). Pure. */
import { m } from "#lib/i18n/m.ts";

const MAX_LISTED = 6;

export interface HostRestartPrompt {
	title: string;
	description: string;
	confirmLabel: string;
	destructive: boolean;
}

/** `~/…` for home-relative paths is left to the caller; this only trims to the last two segments. */
const shortCwd = (cwd: string) => cwd.split("/").filter(Boolean).slice(-2).join("/") || cwd;

export function hostRestartPrompt(ptys: { title: string; cwd: string }[]): HostRestartPrompt {
	if (ptys.length === 0)
		return {
			title: m.workbench_host_prompt_title(),
			description: m.workbench_host_prompt_no_terminals(),
			confirmLabel: m.workbench_host_prompt_restart_host(),
			destructive: false,
		};
	const listed = ptys.slice(0, MAX_LISTED).map((p) => `• ${p.title || m.crew_chat_header_terminal()} · ${shortCwd(p.cwd)}`);
	const more = ptys.length > MAX_LISTED ? [m.workbench_host_more_terminals({ count: ptys.length - MAX_LISTED })] : [];
	const n = ptys.length;
	return {
		title: m.workbench_host_prompt_title(),
		description: [
			m.workbench_host_prompt_ends({ count: n }),
			"",
			...listed,
			...more,
			"",
			m.workbench_host_prompt_language_features(),
		].join("\n"),
		confirmLabel: m.workbench_host_prompt_end_and_restart({ count: n }),
		destructive: true,
	};
}
