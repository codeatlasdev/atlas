import { m } from "#lib/i18n/m.ts";

/**
 * Which commands the palette lists under "Recent commands", and in what order. Pure: the registry and the recents are
 * the caller's.
 *
 * The recents record every command that ran (the onboarding checklist ticks "Try the command palette" from it), but
 * opening the palette from the palette is never what someone wants to repeat: 1.5.0-alpha.26 listed "Command palette"
 * itself under "Recent commands" on a first run.
 */
const NOT_LISTED_AS_RECENT = new Set(["command_palette::Toggle"]);

/** Recent ids in the order to show them (newest first), without the commands that are not worth repeating. */
export function listedRecents(recentIds: readonly string[]): string[] {
	return recentIds.filter((id) => !NOT_LISTED_AS_RECENT.has(id));
}

/** `items` tagged with their group, recent ones first in recency order, the rest after in their given order. */
export function groupCommands<T extends { id: string }>(
	items: readonly T[],
	recentIds: readonly string[],
): (T & { group: string })[] {
	const listed = listedRecents(recentIds);
	const rank = (id: string) => {
		const i = listed.indexOf(id);
		return i < 0 ? Infinity : i;
	};
	return items
		.map((c) => ({
			...c,
			group: rank(c.id) === Infinity ? m.shell_commands_group_commands() : m.shell_commands_group_recent_commands(),
		}))
		.sort((a, b) => rank(a.id) - rank(b.id));
}
