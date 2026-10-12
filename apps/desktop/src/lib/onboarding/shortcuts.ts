/** Cheat sheet model: bound commands grouped by area, built from the live keymap. */

export interface SheetRow {
	id: string;
	label: string;
	keys: string[];
}

export interface SheetGroup {
	title: string;
	rows: SheetRow[];
}

const GROUPS: { title: string; match: (id: string) => boolean }[] = [
	{
		title: "Go anywhere",
		match: (id) =>
			["atlas::Jump", "command_palette::Toggle", "file_finder::Toggle", "outline::Toggle"].includes(id) ||
			id.startsWith("atlas::RecentWorktree"),
	},
	{
		title: "Panels",
		match: (id) =>
			id.startsWith("workspace::Toggle") ||
			id.endsWith("::ToggleFocus") ||
			id === "agent::Toggle" ||
			id.startsWith("shell::"),
	},
	{
		title: "Tabs and panes",
		match: (id) => id.startsWith("pane::") || id === "workspace::NewTerminal" || id === "atlas::CloseEditors",
	},
	{
		title: "Project",
		match: (id) =>
			id.startsWith("atlas::") && !id.startsWith("atlas::RecentWorktree") && id !== "atlas::Jump",
	},
];

const OTHER = "More";

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
				? "Recent worktree 1–6"
				: c.id === "pane::ActivateItem1"
					? "Tab 1–8 (⌘9 last)"
					: c.label;
		if (q && !label.toLowerCase().includes(q) && !keys.some((k) => k.toLowerCase().includes(q))) continue;
		const title = GROUPS.find((g) => g.match(c.id))?.title ?? OTHER;
		const rows = out.get(title) ?? [];
		rows.push({ id: c.id, label, keys });
		out.set(title, rows);
	}
	const order = [...GROUPS.map((g) => g.title), OTHER];
	return order.filter((t) => out.has(t)).map((title) => ({ title, rows: out.get(title)! }));
}
