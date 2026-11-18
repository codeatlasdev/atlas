/** Cheat sheet model: bound commands grouped by area, built from the live keymap. */
import { m } from "#lib/i18n/m.ts";
import { kbd } from "#lib/keys.ts";

interface SheetRow {
	id: string;
	label: string;
	keys: string[];
}

export interface SheetGroup {
	title: string;
	rows: SheetRow[];
}

/** A group has a stable id (the order and the lookup key) and a title in the display language, read when drawn. */
const GROUPS: { id: string; title: () => string; match: (id: string) => boolean }[] = [
	{
		id: "go",
		title: () => m.onboarding_shortcuts_group_go_anywhere(),
		match: (id) =>
			["atlas::Jump", "command_palette::Toggle", "file_finder::Toggle", "outline::Toggle"].includes(id) ||
			id.startsWith("atlas::RecentWorktree"),
	},
	{
		id: "panels",
		title: () => m.onboarding_shortcuts_group_panels(),
		match: (id) =>
			id.startsWith("workspace::Toggle") ||
			id.endsWith("::ToggleFocus") ||
			id === "agent::Toggle" ||
			id.startsWith("shell::"),
	},
	{
		id: "tabs",
		title: () => m.onboarding_shortcuts_group_tabs(),
		match: (id) => id.startsWith("pane::") || id === "workspace::NewTerminal" || id === "atlas::CloseEditors",
	},
	{
		id: "project",
		title: () => m.onboarding_shortcuts_group_project(),
		match: (id) =>
			id.startsWith("atlas::") && !id.startsWith("atlas::RecentWorktree") && id !== "atlas::Jump",
	},
];

const OTHER = { id: "more", title: () => m.onboarding_shortcuts_group_more() };

/**
 * Group `commands` that have at least one key. Rows keep the order of `commands`;
 * commands from `RecentWorktree2…6` and `ActivateItem2…8` collapse into the first so the list stays short.
 */
export function buildSheet(
	commands: { id: string; label: string }[],
	keysOf: (id: string) => string[],
	query = "",
): SheetGroup[] {
	const q = query.trim().toLowerCase();
	const out = new Map<string, SheetRow[]>();
	for (const c of commands) {
		const keys = keysOf(c.id);
		if (!keys.length) continue;
		if (/^atlas::RecentWorktree[2-9]$/.test(c.id) || /^pane::ActivateItem[2-8]$/.test(c.id)) continue;
		const label =
			c.id === "atlas::RecentWorktree1"
				? m.onboarding_shortcuts_recent_worktree()
				: c.id === "pane::ActivateItem1"
					? m.onboarding_shortcuts_tab_range({ key: kbd("cmd-9") })
					: c.label;
		if (q && !label.toLowerCase().includes(q) && !keys.some((k) => k.toLowerCase().includes(q))) continue;
		const group = GROUPS.find((g) => g.match(c.id)) ?? OTHER;
		const rows = out.get(group.id) ?? [];
		rows.push({ id: c.id, label, keys });
		out.set(group.id, rows);
	}
	return [...GROUPS, OTHER]
		.filter((g) => out.has(g.id))
		.map((g) => ({ title: g.title(), rows: out.get(g.id)! }));
}
