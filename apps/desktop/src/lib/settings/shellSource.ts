/**
 * Why a shell was chosen, as `atlas_core::login_env::resolve_shell` names it (`ShellInfo.probe.source`). Each known
 * value has a message; any other shows as it came. `shellSource.test.ts` reads the list the Rust side writes
 * (`__fixtures__/shell-sources.json`) and fails when a source has no message. Pure: no Svelte, no IPC.
 */
import { m } from "#lib/i18n/m.ts";

export function sourceText(source: string): string {
	switch (source) {
		case "settings":
			return m.settings_terminal_shell_source_settings();
		case "account":
			return m.settings_terminal_shell_source_account();
		case "$SHELL":
			return m.settings_terminal_shell_source_env();
		case "default":
			return m.settings_terminal_shell_source_default();
		default:
			return source;
	}
}
