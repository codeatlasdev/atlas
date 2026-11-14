/**
 * The four command surfaces of the shell (jump, commands, go-to-file, go-to-symbol): their
 * open state and the item lists each `<Palette>` renders.
 */
import type { PaletteItem } from "#lib/components/Palette.svelte";
import { openService, runAction } from "#lib/actions.svelte.ts";
import { fileIcon } from "#lib/icons.ts";
import { allCommands, noteRun, recent, shortcutFor } from "#lib/keymap.svelte.ts";
import { activeEditor } from "#lib/shell/editors.ts";
import { agents } from "#lib/stores/agents.svelte.ts";
import { worktreeFiles, openFile } from "#lib/stores/files.svelte.ts";
import { store } from "#lib/stores/projects.svelte.ts";
import { ws } from "#lib/stores/workspace.svelte.ts";
import { worktrees } from "#lib/stores/worktrees.svelte.ts";
import { lspSymbols } from "#lib/lsp/symbols.svelte.ts";
import { ops } from "#lib/symbols.ts";
import { goTab, goWorktree, recentWorktrees, wtLabel } from "#lib/shell/navigate.ts";

/** Open flags for each palette plus the file list the "Go to file" palette searches. */
export const palettes = $state({
	jump: false,
	/** The Atlas Crew palette (crewmates + crew commands); ⌘K opens it instead of Jump while the Crew workspace is shown. */
	crew: false,
	commands: false,
	files: false,
	symbols: false,
	quickFiles: [] as string[],
});

/** ⌘P: load the selected worktree's files, then open the palette. */
export async function openQuick() {
	const w = ws.selected;
	if (!w) return;
	palettes.quickFiles = await worktreeFiles(w.id);
	palettes.files = true;
}

/** Commands hidden from the palette: numbered variants that only exist for key chords. */
const KEY_ONLY = /^(atlas::RecentWorktree|shell::Space\d|pane::ActivateItem\d)/;

/** Registry commands (recent first) plus the current project's services and actions. */
export function commandItems(): PaletteItem[] {
	const rank = (id: string) => {
		const i = recent.ids.indexOf(id);
		return i < 0 ? Infinity : i;
	};
	const items: PaletteItem[] = allCommands()
		.filter((c) => !KEY_ONLY.test(c.id))
		.map((c) => ({
			id: c.id,
			group: recent.ids.includes(c.id) ? "Recent commands" : "Commands",
			label: c.label,
			shortcut: shortcutFor(c.id),
			run: () => (noteRun(c.id), c.run()),
		}))
		.sort((a, b) => rank(a.id) - rank(b.id));
	const cur = store.current;
	if (!cur) return items;
	for (const s of store.configs[cur.id]?.services.filter((s) => s.target) ?? [])
		items.push({
			id: `svc:${s.name}`,
			group: "Project",
			label: s.target!.how === "browser" ? `Open ${s.name}` : `Copy ${s.name} URL`,
			run: () => void openService(s),
		});
	if (cur.running)
		for (const a of store.configs[cur.id]?.actions.filter((a) => !a.session) ?? [])
			items.push({
				id: `action:${a.key}`,
				group: "Project",
				label: a.label,
				run: () => runAction(cur.id, a.key),
			});
	return items;
}

/** Jump palette: recent terminals and worktrees, every worktree by repo, then the commands. */
export function jumpItems(): PaletteItem[] {
	const items: PaletteItem[] = [];
	const recentTabs = recentWorktrees()
		.flatMap((w) => (ws.doc.recentTabs[w.id] ?? []).map((id) => ws.tab(id)))
		.filter((t) => t && t.kind === "terminal")
		.slice(0, 6);
	recentTabs.forEach((t) =>
		items.push({
			id: t!.id,
			group: "Recent chats & terminals",
			label: (t!.pty && ws.titles[t!.pty]) || t!.label,
			description: wtLabel(t!.worktreeId),
			run: (split) => goTab(t!.id, split),
		}),
	);
	for (const w of recentWorktrees().slice(0, 6))
		items.push({
			id: w.id,
			group: "Recent worktrees",
			project: w.repoId,
			label: wtLabel(w.id),
			description: w.path,
			run: () => goWorktree(w.id),
		});
	for (const r of store.sorted.filter((p) => !p.parent))
		for (const p of [r, ...store.projects.filter((x) => x.parent === r.id)])
			items.push({
				id: p.id,
				group: r.name,
				project: r.id,
				label: p.parent ? (ws.worktree(p.id)?.name ?? p.root) : "main",
				description: p.branch ?? p.root,
				run: () => goWorktree(p.id),
			});
	items.push(...commandItems());
	return items;
}

/** Typed text offered as a new worktree name in the project of the current selection. */
export function createItems(q: string): PaletteItem[] {
	if (!q) return [];
	const repoId = store.current?.parent ?? store.current?.id;
	if (!repoId) return [];
	return [
		{
			id: "create",
			group: "Create",
			label: `Create worktree "${q}"`,
			description: store.projects.find((p) => p.id === repoId)?.name ?? "",
			run: () => void worktrees.create({ repoId, name: q, agent: agents.defaultAgent }),
		},
	];
}

/** "Go to file": the cached file list of the selected worktree; ⇧ opens in a split. */
export function fileItems(): PaletteItem[] {
	const w = ws.selected;
	if (!w) return [];
	return palettes.quickFiles.map((f) => {
		const i = f.lastIndexOf("/");
		return {
			id: f,
			group: "Files",
			label: f.slice(i + 1),
			icon: fileIcon(f.slice(i + 1)),
			description: i > 0 ? f.slice(0, i) : undefined,
			run: (split: boolean) => {
				if (split) {
					const g = ws.split(ws.focusedGroup(w.id).id, "h");
					if (g) ws.focusGroup(g.id);
				}
				openFile(w.id, f);
			},
		};
	});
}

/** "Go to symbol": the outline of the focused pane's editor. */
export function symbolItems(): PaletteItem[] {
	const view = activeEditor();
	if (!view) return [];
	// Language-server symbols (with containers) when the server answered, else the syntax tree's.
	const symbols =
		lspSymbols.view === view && lspSymbols.list
			? lspSymbols.list.map((s) => ({ ...s, kind: s.container ? `${s.kind} in ${s.container}` : s.kind }))
			: ops.outline(view.state);
	return symbols.map((s, i) => ({
		id: `${i}`,
		group: "Symbols",
		label: s.name,
		description: `${s.kind} · line ${s.line}`,
		run: () => {
			ops.reveal(view, s.from);
			view.focus();
		},
	}));
}
