<script lang="ts">
	import {
		X,
		SquareTerminal,
		LayoutGrid,
		ScrollText,
		FileCode2,
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
	import AgentGlyph from "#lib/components/AgentGlyph.svelte";
	import AgentLogo from "#lib/components/AgentLogo.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { host } from "#lib/host.ts";
	import { quick } from "#lib/stores/quick.svelte.ts";
	import { shortcutFor } from "#lib/keymap.svelte.ts";
	import Lazy from "#lib/components/Lazy.svelte";
	import { fileIcon } from "#lib/icons.ts";
	import { tabSwipe } from "#lib/shell/gestures.ts";
	import { pushZones } from "#lib/shell/nativeSwipe.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import Contain from "#lib/errors/Contain.svelte";
	import CrashProbe from "#lib/errors/CrashProbe.svelte";
	import NewTabMenu from "./NewTabMenu.svelte";
	import TabStrip from "./TabStrip.svelte";
	import PaneNotice from "./PaneNotice.svelte";

	let { groupId }: { groupId: string } = $props();
	const space = useSpace();
	let g = $derived(ws.group(groupId));
	let tabs = $derived((g?.order ?? []).map((id) => ws.tab(id)).filter((t): t is Tab => !!t));
	let active = $derived(ws.tab(g?.activeTab));
	// Focus and native views belong to the live space: a mounted neighbour (or a space mid-swipe) never steals them.
	let focused = $derived(!!g && space.live && ws.doc.focusedGroup[g.worktreeId] === groupId);
	/** Tabs shown at least once stay mounted; the rest of a space's tabs mount only once it is current. */
	let seen = $state<Record<string, true>>({});
	/** How many of the not-yet-shown tabs of the current space are mounted (grows one per tick). */
	let extra = $state(0);
	$effect(() => {
		const id = g?.activeTab;
		if (id && !seen[id]) seen[id] = true;
	});
	// The native swipe zone of this group says which neighbours exist (`can` reads `order` and `activeTab` inside
	// closures, which the `use:swipe` argument does not track): tell the engine again whenever they change.
	$effect(() => {
		void [g?.order.length, g?.activeTab];
		pushZones();
	});
	// A space that became current fills in its other tabs one per timer tick: they are hidden, so
	// nothing visible changes, and a 19-editor space never costs a frame.
	$effect(() => {
		if (!space.current || extra >= tabs.length) return;
		const h = setTimeout(() => extra++, 40);
		return () => clearTimeout(h);
	});
	let mounted = $derived.by(() => {
		const rest = space.current ? extra : 0;
		let n = 0;
		return new Set(
			tabs.filter((t) => t.id === g?.activeTab || seen[t.id] || (!!rest && n++ < rest)).map((t) => t.id),
		);
	});
	let w = $derived(ws.worktree(g?.worktreeId));
	let project = $derived(store.projects.find((p) => p.id === w?.repoId));
	let dropZone = $state<"right" | "bottom" | "center" | null>(null);
	let renaming = $state<string | null>(null);
	let groupEl = $state<HTMLElement | null>(null);

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
</script>

{#if g}
	<div
		class="group"
		bind:this={groupEl}
		class:focused
		role="group"
		onpointerdown={() => ws.focusGroup(groupId)}
		ondragover={onDragOver}
		ondragleave={() => (dropZone = null)}
		ondrop={onDrop}
	>
		<TabStrip
			activeTab={g.activeTab ?? undefined}
			count={tabs.length}
			swipeOptions={tabSwipe(
				groupId,
				() => groupEl,
				() => g?.order ?? [],
				() => g?.activeTab ?? undefined,
			)}
		>
			{#snippet tabList()}
				{#each tabs as t (t.id)}
					{@const Icon = ICONS[t.kind as keyof typeof ICONS] ?? SquareTerminal}
					<div
						class="tab"
						data-tab-id={t.id}
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
						{#if t.agent}<AgentLogo agent={t.agent} size={13} /><AgentGlyph
								state={agents.status[t.id]?.state ?? "idle"}
							/>{:else if t.kind === "editor" && t.path}<span class="ico" data-contrast-decorative
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
			{/snippet}
			{#snippet actions()}
				<NewTabMenu {groupId} onrename={(id) => (renaming = id)} />
				{#each quick.for(w?.repoId) as q (q.id)}
					<button class="quick" title={q.command} onclick={() => w && quick.run(q, w.id, groupId)}>
						{#if q.kind === "prompt"}<Bot size={12} />{:else}<Zap size={12} />{/if}
						{q.label}
					</button>
				{/each}
			{/snippet}
		</TabStrip>
		<div class="body">
			{#each tabs as t (t.id)}
				{#if mounted.has(t.id)}
					<div class="pane" data-tab={t.id} class:hidden={t.id !== g.activeTab}>
						<Contain
							where="Tab"
							kind={t.kind}
							file={t.path?.split("/").pop()}
							title={`${label(t)} hit an error`}
							hint="This tab stopped rendering. Your other tabs and terminals are unaffected."
							close={pinned(t) ? undefined : () => ws.closeTab(t.id)}
						>
							<CrashProbe id={t.id} />
							{#if t.chat && t.pty && !t.rawView && !ws.ended(t)}
								<Lazy
									load={() => import("#lib/components/ChatTab.svelte")}
									props={{ tab: t, focused: focused && t.id === g.activeTab }}
								/>
							{:else if t.kind === "terminal" && t.pty}
								<Lazy
									load={() => import("./TerminalPane.svelte")}
									props={{ t, pty: t.pty, active: t.id === g.activeTab && space.live, focused }}
								/>
							{:else if t.kind === "editor" && t.path}
								<Lazy
									load={() => import("#lib/components/Editor.svelte")}
									props={{
										worktreeId: t.worktreeId,
										path: t.path,
										focused: focused && t.id === g.activeTab,
									}}
								/>
							{:else if t.kind === "diff"}
								<Lazy
									load={() => import("#lib/components/DiffView.svelte")}
									props={{ worktreeId: t.worktreeId, focused: focused && t.id === g.activeTab }}
								/>
							{:else if t.kind === "browser"}
								<Lazy
									load={() => import("#lib/components/BrowserTab.svelte")}
									props={{ tab: t, visible: t.id === g.activeTab && space.live }}
								/>
							{:else if t.kind === "pr"}
								<Lazy
									load={() => import("#lib/components/PrTab.svelte")}
									props={{ worktreeId: t.worktreeId, focused: focused && t.id === g.activeTab }}
								/>
							{:else if project && t.kind === "services"}
								<Lazy load={() => import("#lib/panel/Panel.svelte")} props={{ project }} />
							{:else if project && t.kind === "logs"}
								<Lazy
									load={() => import("#lib/components/Logs.svelte")}
									props={{ project, worktreeId: t.worktreeId }}
								/>
							{:else if project && t.kind === "config"}
								<Lazy load={() => import("#lib/components/Config.svelte")} props={{ project }} />
							{:else}
								<PaneNotice>Nothing to show</PaneNotice>
							{/if}
						</Contain>
					</div>
				{/if}
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
	.x {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border: 0;
		border-radius: 4px;
		background: none;
		color: inherit;
		transition:
			opacity 120ms ease-out,
			background 120ms ease-out;
	}
	.tab:not(:hover):not(.on):not(:focus-within) .x {
		opacity: 0;
	}
	.x:hover {
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
	/* A hidden tab stays laid out (so CodeMirror and the terminal slot keep their size and showing one
	   needs no layout pass), just invisible, out of the tab order and out of the way. */
	.pane.hidden {
		position: absolute;
		inset: 0;
		visibility: hidden;
		pointer-events: none;
		contain: strict;
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
