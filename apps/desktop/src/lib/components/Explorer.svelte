<script lang="ts">
	import { onMount, tick } from "svelte";
	import { primaryKey } from "#lib/keys.ts";
	import { listen } from "@tauri-apps/api/event";
	import { Skeleton, toast } from "@codeatlasdev/ui";
	import PointerMenu from "#lib/components/PointerMenu.svelte";
	import { fromMenu } from "#lib/components/rowEvent.ts";
	import { ChevronRight, ChevronDown, Ellipsis, FilePlus, FolderPlus, LocateFixed } from "@lucide/svelte";
	import Tip from "#lib/components/Tip.svelte";
	import { fileIcon, folderIcon } from "#lib/icons.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { host, type FsEntry } from "#lib/host.ts";
	import { git, errText } from "#lib/api.ts";
	import { duplicateName, menuItems, type MenuAction, type MenuTarget } from "#lib/stores/explorerMenu.ts";
	import { uid, ws } from "#lib/stores/workspace.svelte.ts";
	import { openFile, openFileToSide } from "#lib/stores/files.svelte.ts";
	import { errors } from "#lib/errors/errors.svelte.ts";
	import { foldersToReload, loadFolder } from "#lib/stores/explorerLoad.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import ProjectIcon from "#lib/shell/ProjectIcon.svelte";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import {
		visibleRows,
		ancestorsOf,
		join,
		parentOf,
		statusMap,
		statusOf,
		navMove,
	} from "#lib/stores/tree.ts";

	let { worktreeId }: { worktreeId: string } = $props();

	let root = $derived(ws.worktree(worktreeId)?.path ?? "");
	let repoId = $derived(ws.worktree(worktreeId)?.repoId ?? null);
	let project = $derived(store.projects.find((p) => p.id === repoId) ?? null);
	/** File open in the focused pane, highlighted in the tree. */
	let activePath = $derived(ws.focusedTab(worktreeId)?.path);
	let children = $state<Record<string, FsEntry[]>>({});
	let expanded = $state<Record<string, boolean>>({ "": true });
	let status = $state<Record<string, string>>({});
	let watchIds: Record<string, string> = {};
	/** Folders whose listing failed because the terminal host was down: listed again on reconnect. */
	const pending = new Set<string>();

	async function load(rel: string) {
		const r = await loadFolder(() => host.fsList(rel ? `${root}/${rel}` : root), {
			// One shared toast for the outage (not one per folder), and the folder is retried on reconnect.
			onHost: (m) => errors.captureHost(m),
			onOther: (m) => toast.error(m),
		});
		if (!r.ok) {
			if (r.retry) pending.add(rel);
			return;
		}
		pending.delete(rel);
		children[rel] = r.entries;
		if (!watchIds[rel]) {
			watchIds[rel] = uid();
			void host.fsWatch(rel ? `${root}/${rel}` : root, watchIds[rel]).catch(() => {});
		}
	}

	async function loadStatus() {
		const s = await git.status(worktreeId).catch(() => null);
		if (!s) return;
		status = statusMap(s.entries);
	}

	onMount(() => {
		void load("");
		void loadStatus();
		let timer: number | undefined;
		// The host came back (an update, a restart, a handoff): the watches died with the old connection.
		const back = listen("host://reconnected", () => {
			watchIds = {};
			for (const rel of foldersToReload(expanded, pending)) void load(rel);
			void loadStatus();
		});
		const off = listen<{ ev: string; id?: string }>("host://event", (e) => {
			if (e.payload.ev !== "fs_event") return;
			const rel = Object.keys(watchIds).find((k) => watchIds[k] === e.payload.id);
			if (rel === undefined) return;
			void load(rel);
			clearTimeout(timer);
			timer = window.setTimeout(loadStatus, 300);
		});
		return () => {
			void off.then((u) => u());
			void back.then((u) => u());
			for (const id of Object.values(watchIds)) void host.fsUnwatch(id).catch(() => {});
		};
	});

	function toggle(rel: string) {
		expanded[rel] = !expanded[rel];
		if (expanded[rel] && !children[rel]) void load(rel);
	}

	/** Row being edited in place: a new file inside `dir`, or a rename of `rel`. */
	let editing = $state<{ mode: "new" | "folder" | "rename"; rel: string; value: string } | null>(null);
	let editInput: HTMLInputElement | undefined = $state();

	async function beginEdit(mode: "new" | "folder" | "rename", rel: string) {
		if (mode !== "rename") {
			expanded[rel] = true;
			if (!children[rel]) await load(rel);
		}
		editing = { mode, rel, value: mode === "rename" ? (rel.split("/").pop() ?? "") : "" };
		await tick();
		editInput?.focus();
		// Select the stem so typing replaces the name but keeps the extension.
		const v = editing?.value ?? "";
		editInput?.setSelectionRange(0, v.includes(".") ? v.lastIndexOf(".") : v.length);
	}

	const newFile = (dir: string) => beginEdit("new", dir);
	const newFolder = (dir: string) => beginEdit("folder", dir);
	const rename = (rel: string) => beginEdit("rename", rel);

	async function commitEdit() {
		const e = editing;
		editing = null;
		const name = e?.value.trim();
		if (!e || !name || name.includes("/")) return;
		if (e.mode === "folder") {
			await host.fsMkdir(`${root}/${join(e.rel, name)}`).catch((err) => toast.error(errText(err)));
			await load(e.rel);
			expanded[join(e.rel, name)] = true;
		} else if (e.mode === "new") {
			const rel = join(e.rel, name);
			await host.fsWrite(`${root}/${rel}`, "").catch((err) => toast.error(errText(err)));
			await load(e.rel);
			openFile(worktreeId, rel);
		} else if (name !== e.rel.split("/").pop()) {
			const dir = parentOf(e.rel);
			await host
				.fsRename(`${root}/${e.rel}`, `${root}/${join(dir, name)}`)
				.catch((err) => toast.error(errText(err)));
			await load(dir);
		}
		focusRow(e.mode === "rename" ? join(parentOf(e.rel), name) : join(e.rel, name));
	}

	function onEditKey(ev: KeyboardEvent) {
		ev.stopPropagation();
		if (ev.key === "Enter") (ev.preventDefault(), void commitEdit());
		else if (ev.key === "Escape") {
			const rel = editing?.rel;
			editing = null;
			if (rel !== undefined) focusRow(rel);
		}
	}

	let listEl: HTMLDivElement | undefined = $state();
	let cursor = $state("");

	function focusRow(rel: string) {
		cursor = rel;
		void tick().then(() => listEl?.querySelector<HTMLElement>(`[data-rel="${CSS.escape(rel)}"]`)?.focus());
	}

	/** Expand every folder above the active file, then scroll to and highlight it. */
	async function revealActive() {
		if (!activePath) return;
		for (const dir of ancestorsOf(activePath)) {
			expanded[dir] = true;
			if (!children[dir]) await load(dir);
		}
		await tick();
		const el = listEl?.querySelector<HTMLElement>(`[data-rel="${CSS.escape(activePath)}"]`);
		el?.scrollIntoView({ block: "center", behavior: "smooth" });
		cursor = activePath;
	}

	function activate(rel: string, isDir: boolean) {
		cursor = rel;
		if (isDir) toggle(rel);
		else openFile(worktreeId, rel);
	}

	function onTreeKey(ev: KeyboardEvent) {
		if (editing || fromMenu(ev)) return;
		if (onTreeContextKey(ev)) return;
		if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
		const move = navMove(ev.key, visibleRows(children, expanded), cursor, expanded);
		if (!move) return;
		ev.preventDefault();
		if (move.toggle !== undefined) toggle(move.toggle);
		if (move.open) activate(move.open.rel, move.open.dir);
		if (move.rename !== undefined) void rename(move.rename);
		if (move.focus !== undefined) focusRow(move.focus);
	}

	async function remove(rel: string, dir: boolean) {
		if (
			!(await confirmDialog(`Delete ${rel.split("/").pop()}?`, {
				description: dir ? "The folder and everything in it." : rel,
				confirmLabel: "Delete",
				destructive: true,
			}))
		)
			return;
		await host.fsRemove(`${root}/${rel}`, dir).catch((e) => toast.error(errText(e)));
		await load(parentOf(rel));
	}

	async function gitDo(args: string[]) {
		const r = await git.run(worktreeId, args);
		if (r.code !== 0) toast.error(r.stderr || "git failed");
		await loadStatus();
	}

	const copy = (t: string) => navigator.clipboard.writeText(t).then(() => toast.success("Copied"));

	/**
	 * One menu for the `…` button, right-click and Shift+F10 / the menu key: `menuTarget` says what it is about,
	 * `menuItems` what it offers, `PointerMenu` where it opens.
	 */
	let menuTarget = $state<MenuTarget>({ kind: "root" });
	let pointerMenu: PointerMenu | undefined = $state();

	async function openMenu(target: MenuTarget, x: number, y: number) {
		menuTarget = target;
		if (target.kind !== "root") cursor = target.rel;
		await tick();
		await pointerMenu?.openAt(x, y);
	}

	function menuOf(rel: string, isDir: boolean): MenuTarget {
		return isDir ? { kind: "dir", rel } : { kind: "file", rel, status: statusOf(status, rel) };
	}

	/** Right-click on a row (`rel` set) or on the empty area (the worktree root). */
	function onContext(ev: MouseEvent, rel?: string, isDir = false) {
		ev.preventDefault();
		ev.stopPropagation();
		void openMenu(rel === undefined ? { kind: "root" } : menuOf(rel, isDir), ev.clientX, ev.clientY);
	}

	/** Shift+F10 / the context-menu key on the focused row: open under it, like a native tree. */
	function onTreeContextKey(ev: KeyboardEvent) {
		if (!(ev.key === "ContextMenu" || (ev.key === "F10" && ev.shiftKey))) return false;
		const row = (ev.target as Element).closest<HTMLElement>("[data-rel]");
		if (!row || row.dataset.rel === undefined) return false;
		ev.preventDefault();
		const r = row.getBoundingClientRect();
		const rel = row.dataset.rel;
		void openMenu(menuOf(rel, row.getAttribute("aria-expanded") !== null), r.left + 24, r.bottom);
		return true;
	}

	async function duplicate(rel: string) {
		const dir = parentOf(rel);
		const name = rel.split("/").pop() ?? rel;
		const taken = new Set((children[dir] ?? []).map((e) => e.name));
		const to = join(dir, duplicateName(name, taken));
		const r = await host.exec(root, ["cp", "-R", "--", `${root}/${rel}`, `${root}/${to}`]).catch((e) => e);
		if (r instanceof Error || (r as { code?: number }).code) {
			toast.error(r instanceof Error ? errText(r) : (r as { stderr: string }).stderr || "Duplicate failed");
			return;
		}
		await load(dir);
		focusRow(to);
	}

	/** Runs one entry of `menuItems`; `t` is the target the menu was opened for. */
	async function runMenu(action: MenuAction, t: MenuTarget) {
		const rel = t.kind === "root" ? "" : t.rel;
		const dir = t.kind === "file" ? parentOf(rel) : rel;
		switch (action) {
			case "open":
				return openFile(worktreeId, rel);
			case "open-side":
				return openFileToSide(worktreeId, rel);
			case "terminal":
				return void ws.newTerminal(worktreeId, { cwd: dir ? `${root}/${dir}` : root });
			case "new-file":
				return newFile(dir);
			case "new-folder":
				return newFolder(dir);
			case "rename":
				return rename(rel);
			case "duplicate":
				return duplicate(rel);
			case "copy-path":
				return copy(rel ? `${root}/${rel}` : root);
			case "copy-relative-path":
				return copy(rel);
			case "reveal":
				return host.exec(root, ["open", "-R", rel ? `${root}/${rel}` : root]);
			case "stage":
				return gitDo(["add", "--", rel]);
			case "discard":
				return (
					(await confirmDialog(`Discard changes to ${rel}?`, {
						confirmLabel: "Discard",
						destructive: true,
					})) && gitDo(["checkout", "--", rel])
				);
			case "delete":
				return remove(rel, t.kind === "dir");
		}
	}

	function onkeydown(e: KeyboardEvent) {
		if (primaryKey(e) && e.altKey && e.shiftKey && e.code === "KeyC") {
			const t = ws.tab(ws.focusedGroup(worktreeId).activeTab);
			if (t?.path) {
				void copy(t.path);
				e.preventDefault();
			}
		}
	}
</script>

<svelte:window {onkeydown} />

{#snippet newRow(dir: string, depth: number)}
	{#if editing && editing.mode !== "rename" && editing.rel === dir}
		<div class="row editing" style:padding-left="{8 + depth * prefs.value.project_panel.indent_size}px">
			<span class="sp"></span>
			<input
				class="edit"
				bind:this={editInput}
				bind:value={editing.value}
				onkeydown={onEditKey}
				onblur={commitEdit}
				placeholder="name.ext"
				spellcheck="false"
				aria-label="New file name"
			/>
		</div>
	{/if}
{/snippet}

{#snippet tree(dir: string, depth: number)}
	{@render newRow(dir, depth)}
	{#each children[dir] ?? [] as e (e.name)}
		{@const rel = join(dir, e.name)}
		{@const isDir = e.kind === "dir"}
		{@const st = statusOf(status, rel)}
		<div
			class="row {st ?? ''}"
			class:active={!isDir && rel === activePath}
			class:cur={cursor === rel}
			style:padding-left="{8 + depth * prefs.value.project_panel.indent_size}px"
			role="treeitem"
			aria-selected={!isDir && rel === activePath}
			aria-expanded={isDir ? !!expanded[rel] : undefined}
			data-rel={rel}
			tabindex={cursor === rel || (!cursor && rel === (children[""]?.[0]?.name ?? "")) ? 0 : -1}
			draggable="true"
			ondragstart={(ev) => ev.dataTransfer?.setData("text/plain", `${root}/${rel}`)}
			onfocus={() => (cursor = rel)}
			onclick={(ev) => fromMenu(ev) || activate(rel, isDir)}
			oncontextmenu={(ev) => onContext(ev, rel, isDir)}
			onkeydown={(ev) => {
				if (ev.key !== " " || fromMenu(ev) || ev.target !== ev.currentTarget) return;
				ev.preventDefault();
				activate(rel, isDir);
			}}
		>
			{#if isDir}
				{#if expanded[rel]}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
				{#if prefs.value.project_panel.folder_icons}<span class="ico" data-contrast-decorative
						>{@html folderIcon(e.name, !!expanded[rel])}</span
					>{/if}
			{:else}
				<span class="sp"></span>{#if prefs.value.project_panel.file_icons}<span
						class="ico"
						data-contrast-decorative>{@html fileIcon(e.name)}</span
					>{/if}
			{/if}
			{#if editing?.mode === "rename" && editing.rel === rel}
				<input
					class="edit"
					bind:this={editInput}
					bind:value={editing.value}
					onkeydown={onEditKey}
					onblur={commitEdit}
					onclick={(ev) => ev.stopPropagation()}
					spellcheck="false"
					aria-label="New name"
				/>
			{:else}
				<span class="name">{e.name}</span>
			{/if}
			<button
				type="button"
				class="more"
				aria-label="More actions"
				tabindex="-1"
				onclick={(ev) => {
					ev.stopPropagation();
					const r = ev.currentTarget.getBoundingClientRect();
					void openMenu(menuOf(rel, isDir), r.right, r.bottom);
				}}
				onkeydown={(ev) => ev.stopPropagation()}><Ellipsis size={12} /></button
			>
		</div>
		{#if isDir && expanded[rel]}{@render tree(rel, depth + 1)}{/if}
	{/each}
{/snippet}

<aside class="explorer">
	<div class="head">
		<span class="title">
			{#if repoId}<ProjectIcon id={repoId} size={16} />{/if}
			<span class="tname">{project?.name ?? "Files"}</span>
		</span>
		<span class="hbtns">
			<Tip label="Reveal active file" side="bottom">
				{#snippet child(props)}
					<button
						{...props}
						class="hbtn"
						aria-label="Reveal active file"
						disabled={!activePath}
						onclick={revealActive}
					>
						<LocateFixed size={13} />
					</button>
				{/snippet}
			</Tip>
			<Tip label="New file" side="bottom">
				{#snippet child(props)}
					<button {...props} class="hbtn" aria-label="New file" onclick={() => newFile("")}>
						<FilePlus size={13} />
					</button>
				{/snippet}
			</Tip>
			<Tip label="New folder" side="bottom">
				{#snippet child(props)}
					<button {...props} class="hbtn" aria-label="New folder" onclick={() => newFolder("")}>
						<FolderPlus size={13} />
					</button>
				{/snippet}
			</Tip>
		</span>
	</div>
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div
		class="list"
		bind:this={listEl}
		onkeydown={onTreeKey}
		oncontextmenu={(ev) => onContext(ev)}
		role="tree"
		tabindex="-1"
	>
		{#if !children[""]}
			<div class="skel" aria-busy="true">
				{#each [70, 55, 80, 45, 65] as w, i (i)}<Skeleton variant="rounded" height={14} width="{w}%" />{/each}
			</div>
		{:else if !children[""].length}
			<p class="none">This folder is empty.</p>
		{:else}
			{@render tree("", 0)}
		{/if}
	</div>
	<PointerMenu
		bind:this={pointerMenu}
		items={menuItems(menuTarget).map((i) => ({ ...i, run: () => runMenu(i.action, menuTarget) }))}
	/>
</aside>

<style>
	.explorer {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		min-height: 0;
		font-size: 12.5px;
	}
	.title {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		min-width: 0;
	}
	.tname {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		height: 32px;
		flex-shrink: 0;
		padding: 0 8px 0 12px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-size: 11px;
		font-weight: 650;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	.hbtn {
		display: inline-grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: inherit;
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.hbtns {
		display: inline-flex;
		gap: 2px;
	}
	.hbtn:disabled {
		color: var(--ca-text-disabled);
		pointer-events: none;
	}
	.hbtn:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.skel {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
	}
	.none {
		margin: 0;
		padding: 16px 12px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.list {
		flex: 1;
		overflow: auto;
		padding: 4px 0 10px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 4px;
		height: 22px;
		padding-right: 6px;
		margin: 0 4px;
		border-radius: 5px;
		cursor: default;
		color: var(--ca-text-primary);
		outline-offset: -1px;
	}
	.row:hover {
		background: var(--ca-action-hover);
	}
	/* Keyboard cursor: only visible during keyboard navigation (lib/native.ts sets data-kbd). */
	:global([data-kbd]) .row.cur:focus {
		box-shadow: inset 0 0 0 1px var(--space-accent, var(--ca-brand-main));
	}
	.row:focus {
		outline: none;
	}
	.edit {
		flex: 1;
		min-width: 0;
		height: 18px;
		padding: 0 4px;
		border: 1px solid var(--space-accent, var(--ca-brand-main));
		border-radius: 4px;
		outline: 0;
		background: var(--ca-surface-overlay, transparent);
		color: var(--ca-text-primary);
		font: inherit;
	}
	.row.active {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 16%, transparent);
	}
	.sp {
		width: 11px;
	}
	.ico {
		display: inline-flex;
		flex-shrink: 0;
	}
	.ico :global(svg) {
		width: 16px;
		height: 16px;
	}
	.name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.modified .name {
		color: var(--atlas-vcs-modified, var(--ca-semantic-warning));
	}
	.untracked .name {
		color: var(--atlas-vcs-added, var(--ca-semantic-success));
	}
	.deleted .name {
		color: var(--atlas-vcs-deleted, var(--ca-semantic-error));
		text-decoration: line-through;
	}
	/* Gitignored (VS Code / Zed): its own dimmed token (quieter than text, AA on every surface: theme/derive.ts) plus
	   italic so it is not only a shade; never a vcs hue, so it cannot be mistaken for untracked or modified. */
	.ignored .name {
		color: var(--atlas-vcs-ignored, var(--ca-text-secondary));
		font-style: italic;
	}
	.ignored .ico {
		opacity: 0.55;
	}
	.more {
		display: inline-flex;
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		cursor: pointer;
		opacity: 0;
	}
	.more:focus-visible,
	.row:hover .more {
		opacity: 1;
	}
</style>
