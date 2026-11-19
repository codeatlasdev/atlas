/**
 * The status chip of an SSH host in Settings > SSH hosts. The desktop shell (`crates/atlas-desktop/src/remote.rs`,
 * `SshStatus::wire`; fixture `__fixtures__/ssh-status.json`) reports one string per host: `connecting`, `connected`,
 * `removed`, `warning: <what is missing>` or `error: <why>`; a host it has not reported on yet has none. The id is
 * mapped to a message here; the text after `warning:` and `error:` is the shell's sentence (ssh's own words inside)
 * and shows raw below the row, and a status this module does not know shows raw.
 */
import { m } from "#lib/i18n/m.ts";

/** The chip's color: `StatusChip`'s `status` prop. */
export type SshTone = "success" | "warning" | "error" | "neutral";

export interface SshStatusView {
	tone: SshTone;
	label: string;
	/** The shell's own text for a warning or an error; absent for the rest. */
	detail?: string;
}

/** The chip label is cut here so a long status never stretches the row. */
export const LABEL_MAX = 60;

/** Read at render time: the words follow the display language. */
export function sshStatusView(raw: string | undefined): SshStatusView {
	if (raw === undefined) return { tone: "neutral", label: m.settings_agents_ssh_status_idle() };
	if (raw === "connected") return { tone: "success", label: m.settings_agents_ssh_status_connected() };
	if (raw === "connecting") return { tone: "warning", label: m.settings_agents_ssh_status_connecting() };
	if (raw === "removed") return { tone: "neutral", label: m.settings_agents_ssh_status_removed() };
	if (raw.startsWith("warning")) {
		return { tone: "warning", label: m.settings_agents_ssh_status_outdated(), detail: raw };
	}
	if (raw.startsWith("error")) {
		return { tone: "error", label: m.settings_agents_ssh_status_error(), detail: raw };
	}
	return { tone: "neutral", label: raw.slice(0, LABEL_MAX) };
}
