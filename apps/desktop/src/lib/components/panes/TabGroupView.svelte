<script lang="ts">
	import { DropdownMenu, DropdownMenuItem, IconButton } from "@codeatlasdev/ui";
	import {
		Plus,
		X,
		SquareTerminal,
		LayoutGrid,
		ScrollText,
		FileCode2,
		RotateCw,
		Bot,
		Zap,
		FileText,
		FileDiff,
		GitPullRequest,
		Globe,
	} from "@lucide/svelte";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { pinned } from "#lib/stores/tabs.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import Terminal from "#lib/components/Terminal.svelte";
	import GpuTerminal from "#lib/components/GpuTerminal.svelte";
	import { gpu } from "#lib/stores/gpu.svelte.ts";
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { host } from "#lib/host.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import { openDiff } from "#lib/stores/files.svelte.ts";
	import { pr } from "#lib/stores/pr.svelte.ts";
	import Panel from "#lib/panel/Panel.svelte";
	import Editor from "#lib/components/Editor.svelte";
	import DiffView from "#lib/components/DiffView.svelte";
	import PrTab from "#lib/components/PrTab.svelte";
	import BrowserTab from "#lib/components/BrowserTab.svelte";
	import ChatTab from "#lib/components/ChatTab.svelte";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import Logs from "#lib/components/Logs.svelte";
	import { fileIcon } from "#lib/icons.ts";
	import Config from "#lib/components/Config.svelte";
	import { swipe } from "#lib/shell/swipe.ts";
	import { tabSwipe } from "#lib/shell/gestures.ts";

	let { groupId }: { groupId: string } = $props();
	let g = $derived(ws.group(groupId));
	let tabs = $derived((g?.order ?? []).map((id) => ws.tab(id)).filter((t): t is Tab => !!t));
	let active = $derived(ws.tab(g?.activeTab));
	let focused = $derived(!!g && ws.doc.focusedGroup[g.worktreeId] === groupId);
	let w = $derived(ws.worktree(g?.worktreeId));
	let project = $derived(store.projects.find((p) => p.id === w?.repoId));
	let dropZone = $state<"right" | "bottom" | "center" | null>(null);
	let renaming = $state<string | null>(null);

	const ICONS = {
		terminal: SquareTerminal,
		services: LayoutGrid,
		logs: ScrollText,
		config: FileCode2,
		editor: FileText,
		diff: FileDiff,
		pr: GitPullRequest,
		browser: Globe,
	} as const;

	function label(t: Tab) {
		if (t.customLabel) return t.label;
		// Tabs saved before the Services tab became the project Panel keep their old stored label.
		if (t.kind === "services") return "Panel";
		const title = t.pty ? ws.titles[t.pty] : "";
		return title || t.label;
	}

	function onDragOver(e: DragEvent) {
		if (!e.dataTransfer?.types.includes("atlas/tab")) {
			if (active?.pty && e.dataTransfer?.types.includes("text/plain")) e.preventDefault();
			return;
		}
		e.preventDefault();
		const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
		dropZone = e.clientX > r.right - 24 ? "right" : e.clientY > r.bottom - 24 ? "bottom" : "center";
	}

	function onDrop(e: DragEvent) {
		const file = e.dataTransfer?.getData("text/plain");
		if (file && !e.dataTransfer?.types.includes("atlas/tab") && active?.pty) {
			e.preventDefault();
			void host.ptyWrite(active.pty, `'${file.replace(/'/g, "'\\''")}' `);
			return;
		}
		const id = e.dataTransfer?.getData("atlas/tab");
		const zone = dropZone;
		dropZone = null;
		if (!id) return;
		e.preventDefault();
		if (zone === "right") ws.split(groupId, "h", id);
		else if (zone === "bottom") ws.split(groupId, "v", id);
		else ws.moveTab(id, groupId);
	}

	function openKind(kind: "services" | "logs" | "config") {
		if (!w) return;
		const labels = { services: "Panel", logs: "Logs", config: "Config" };
		ws.reveal(w.id, (t) => t.kind === kind, { kind, label: labels[kind] });
	}
</script>

{#if g}
	<div
		class="group"
		class:focused
		role="group"
		onpointerdown={() => ws.focusGroup(groupId)}
		ondragover={onDragOver}
		ondragleave={() => (dropZone = null)}
		ondrop={onDrop}
	>
		<div
			class="strip"
			data-gesture-zone
			use:swipe={tabSwipe(
				() => g?.order ?? [],
				() => g?.activeTab ?? undefined,
			)}
		>
			{#each tabs as t (t.id)}
				{@const Icon = ICONS[t.kind as keyof typeof ICONS] ?? SquareTerminal}
				<div
					class="tab"
					class:on={t.id === g.activeTab}
					class:pinned={pinned(t)}
					role="tab"
					tabindex="0"
					aria-selected={t.id === g.activeTab}
					aria-label={pinned(t) ? `${label(t)} (always open)` : undefined}
					title={pinned(t) ? `${label(t)} · always open` : undefined}
					draggable={!pinned(t)}
					ondragstart={(e) => e.dataTransfer?.setData("atlas/tab", t.id)}
					onclick={() => ws.activate(t.id)}
					onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), ws.activate(t.id))}
					onauxclick={(e) => e.button === 1 && ws.closeTab(t.id)}
					ondblclick={() => !pinned(t) && (renaming = t.id)}
				>
					{#if t.agent}<AgentGlyph
							state={agents.status[t.id]?.state ?? "idle"}
						/>{:else if t.kind === "editor" && t.path}<span class="ico"
							>{@html fileIcon(t.path.split("/").pop() ?? t.label)}</span
						>{:else}<Icon size={13} />{/if}
					{#if renaming === t.id}
						<!-- svelte-ignore a11y_autofocus -->
						<input
							class="rename"
							autofocus
							value={label(t)}
							onblur={() => (renaming = null)}
							onkeydown={(e) => {
								if (e.key === "Enter") {
									const v = (e.currentTarget as HTMLInputElement).value.trim();
									if (v) {
										t.label = v;
										t.customLabel = true;
									}
									renaming = null;
								} else if (e.key === "Escape") renaming = null;
							}}
						/>
					{:else if !pinned(t)}
						<span class="label">{label(t)}</span>
					{/if}
					{#if !pinned(t)}
						<button
							class="x"
							aria-label="Close tab"
							title="Close tab {shortcutFor('pane::CloseActiveItem') ?? ''}"
							onclick={(e) => (e.stopPropagation(), ws.closeTab(t.id))}
						>
							<X size={12} />
						</button>
					{/if}
				</div>
			{/each}
			<DropdownMenu align="start">
				{#snippet trigger()}
					<span
						class="plus"
						aria-label="New tab"
						title="New tab {shortcutFor('workspace::NewTerminal') ?? ''}"><Plus size={14} /></span
					>
				{/snippet}
				<DropdownMenuItem onclick={() => w && ws.newTerminal(w.id, { groupId })}>
					<SquareTerminal size={14} /> Terminal
					<span class="kbd">{shortcutFor("workspace::NewTerminal") ?? ""}</span>
				</DropdownMenuItem>
				{#each agents.list.filter((a) => a.installed) as a (a.id)}
					<DropdownMenuItem onclick={() => w && agents.launch(w.id, a.id, { groupId })}>
						<Bot size={14} /> New {a.label} session
					</DropdownMenuItem>
				{/each}
				{#if agents.list.find((a) => a.id === "omp" && a.installed)}
					<DropdownMenuItem onclick={() => w && agents.launch(w.id, "omp", { groupId, chat: true })}>
						<Bot size={14} /> New omp chat
					</DropdownMenuItem>
				{/if}
				{#if project}
					<DropdownMenuItem onclick={() => openKind("services")}
						><LayoutGrid size={14} /> Panel</DropdownMenuItem
					>
					<DropdownMenuItem onclick={() => openKind("logs")}><ScrollText size={14} /> Logs</DropdownMenuItem>
					<DropdownMenuItem onclick={() => openKind("config")}
						><FileCode2 size={14} /> Config</DropdownMenuItem
					>
				{/if}
				{#if w}
					<DropdownMenuItem onclick={() => openDiff(w.id)}><FileDiff size={14} /> Changes</DropdownMenuItem>
					<DropdownMenuItem onclick={() => pr.openTab(w.id)}
						><GitPullRequest size={14} /> Pull request</DropdownMenuItem
					>
				{/if}
				<DropdownMenuItem onclick={() => g.activeTab && ws.split(groupId, "h", g.activeTab)}
					>Split right <span class="kbd">{shortcutFor("pane::SplitRight") ?? ""}</span></DropdownMenuItem
				>
				<DropdownMenuItem onclick={() => g.activeTab && ws.split(groupId, "v", g.activeTab)}
					>Split down <span class="kbd">{shortcutFor("pane::SplitDown") ?? ""}</span></DropdownMenuItem
				>
				{#if active}
					{#if !pinned(active)}<DropdownMenuItem onclick={() => (renaming = active.id)}
							>Rename tab</DropdownMenuItem
						>{/if}
					<DropdownMenuItem onclick={() => ws.closeMany(groupId, active.id)}>
						Close others <span class="kbd">{shortcutFor("pane::CloseOtherItems") ?? ""}</span>
					</DropdownMenuItem>
					<DropdownMenuItem onclick={() => ws.closeMany(groupId)}>
						Close all <span class="kbd">{shortcutFor("pane::CloseAllItems") ?? ""}</span>
					</DropdownMenuItem>
				{/if}
			</DropdownMenu>
			{#each quick.for(w?.repoId) as q (q.id)}
				<button class="quick" title={q.command} onclick={() => w && quick.run(q, w.id, groupId)}>
					{#if q.kind === "prompt"}<Bot size={12} />{:else}<Zap size={12} />{/if}
					{q.label}
				</button>
			{/each}
			<div class="fill" data-tauri-drag-region></div>
		</div>
		<div class="body">
			{#each tabs as t (t.id)}
				<div class="pane" class:hidden={t.id !== g.activeTab}>
					{#if t.chat && t.pty && !t.rawView && !ws.ended(t)}
						<ChatTab tab={t} focused={focused && t.id === g.activeTab} />
					{:else if t.kind === "terminal" && t.pty}
						{@const ended = ws.ended(t)}
						{#if t.chat && !ended}
							<button class="ended" onclick={() => (t.rawView = false)}
								>Raw omp protocol · back to chat</button
							>
						{/if}
						{#if ended}
							<div class="ended">
								<span
									>{ended === "gone"
										? "Terminal is gone"
										: `Process ended (${ws.exited[t.pty] ?? "?"})`}</span
								>
								<IconButton size="sm" variant="text" aria-label="Restart" onclick={() => ws.restart(t.id)}>
									<RotateCw size={13} />
								</IconButton>
								<span class="hint">Restart</span>
							</div>
						{/if}
						{#if ended !== "gone"}
							{#key t.pty}
								{#if gpu.terminal && !gpu.hostTooOld && !ws.worktree(t.worktreeId)?.hostId}
									<GpuTerminal
										tab={t.id}
										pty={t.pty}
										visible={t.id === g.activeTab}
										focused={focused && t.id === g.activeTab}
										readonly={ended !== null}
										onLink={(url, external) => openPreview(t.worktreeId, url, external)}
									/>
								{:else}
									<Terminal
										pty={t.pty}
										focused={focused && t.id === g.activeTab}
										readonly={ended !== null}
										onLink={(url, external) => openPreview(t.worktreeId, url, external)}
									/>
								{/if}
							{/key}
						{/if}
					{:else if t.kind === "editor" && t.path}
						<Editor worktreeId={t.worktreeId} path={t.path} focused={focused && t.id === g.activeTab} />
					{:else if t.kind === "diff"}
						<DiffView worktreeId={t.worktreeId} focused={focused && t.id === g.activeTab} />
					{:else if t.kind === "browser"}
						<BrowserTab tab={t} visible={t.id === g.activeTab} />
					{:else if t.kind === "pr"}
						<PrTab worktreeId={t.worktreeId} focused={focused && t.id === g.activeTab} />
					{:else if project && t.kind === "services"}
						<Panel {project} />
					{:else if project && t.kind === "logs"}
						<Logs {project} worktreeId={t.worktreeId} />
					{:else if project && t.kind === "config"}
						<Config {project} />
					{:else}
						<div class="ended">Nothing to show</div>
					{/if}
				</div>
			{/each}
			{#if dropZone}<div class="drop {dropZone}"></div>{/if}
		</div>
	</div>
{/if}

<style>
	.group {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	.strip {
		display: flex;
		align-items: center;
		gap: 2px;
		height: 32px;
		padding: 0 6px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		overflow-x: auto;
		scrollbar-width: none;
	}
	.ico {
		display: inline-flex;
		flex-shrink: 0;
	}
	.ico :global(svg) {
		width: 16px;
		height: 16px;
	}
	.tab {
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 24px;
		max-width: 220px;
		padding: 0 4px 0 8px;
		border-radius: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: default;
		flex-shrink: 0;
	}
	.tab.pinned {
		width: 28px;
		padding: 0;
		justify-content: center;
	}
	.tab:hover {
		background: var(--ca-action-hover);
	}
	.tab.on {
		color: var(--ca-text-primary);
		background: var(--ca-action-selected, var(--ca-action-hover));
	}
	.group:not(.focused) .tab.on {
		background: var(--ca-action-hover);
	}
	/* Zed `active_pane_modifiers`: set from settings (`--atlas-pane-*`). */
	.group:not(.focused) {
		opacity: var(--atlas-pane-inactive-opacity, 1);
	}
	.group.focused {
		box-shadow: inset 0 0 0 var(--atlas-pane-active-border, 0) var(--ca-brand-main);
	}
	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.rename {
		font: inherit;
		width: 120px;
	}
	.x,
	.plus {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border: 0;
		border-radius: 4px;
		background: none;
		color: inherit;
		opacity: 0.6;
	}
	.plus {
		width: 24px;
		height: 24px;
	}
	.x,
	.plus {
		transition:
			opacity 120ms ease-out,
			background 120ms ease-out;
	}
	.tab:not(:hover):not(.on):not(:focus-within) .x {
		opacity: 0;
	}
	.x:hover,
	.plus:hover {
		opacity: 1;
		background: var(--ca-action-hover);
	}
	.quick {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: 22px;
		padding: 0 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 6px;
		background: none;
		font: inherit;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		flex-shrink: 0;
	}
	.quick:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.fill {
		flex: 1;
		align-self: stretch;
	}
	.kbd {
		margin-left: auto;
		color: var(--ca-text-disabled);
		font-size: 11px;
	}
	.body {
		position: relative;
		flex: 1;
		display: flex;
		min-height: 0;
	}
	.pane {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	.pane.hidden {
		display: none;
	}
	.ended {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		font-size: 12px;
		color: var(--ca-text-secondary);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.hint {
		color: var(--ca-text-disabled);
	}
	.drop {
		position: absolute;
		pointer-events: none;
		background: color-mix(in oklab, var(--ca-brand-main) 18%, transparent);
		border: 1px solid var(--ca-brand-main);
		border-radius: 6px;
	}
	.drop.center {
		inset: 4px;
	}
	.drop.right {
		top: 4px;
		bottom: 4px;
		right: 4px;
		width: 50%;
	}
	.drop.bottom {
		left: 4px;
		right: 4px;
		bottom: 4px;
		height: 50%;
	}
</style>
