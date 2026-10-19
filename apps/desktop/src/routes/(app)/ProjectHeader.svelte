<script lang="ts">
	import {
		Button,
		DropdownMenu,
		DropdownMenuItem,
		DropdownMenuLabel,
		Dialog,
		NotificationBell,
		Spinner,
	} from "@codeatlasdev/ui";
	import {
		ChevronDown,
		Code,
		Ellipsis,
		PanelBottom,
		PanelLeft,
		Pin,
		PinOff,
		Play,
		Square,
		Trash2,
	} from "@lucide/svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import Tip from "#lib/components/Tip.svelte";
	import PanelOps from "#lib/panel/PanelOps.svelte";
	import Crumbs from "#lib/shell/Crumbs.svelte";
	import RightToggle from "#lib/shell/RightToggle.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { docks, toggleDock } from "#lib/stores/docks.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { summarize } from "#lib/panel/metrics.ts";

	let { p }: { p: ProjectView } = $props();

	let live = $derived(store.live[p.id]);
	let busy = $derived(!!store.busy[`stack:${p.id}`]);
	let cfg = $derived(store.configs[p.id]);
	let health = $derived(
		summarize(
			(p.running && live?.services.length ? live.services : (cfg?.services ?? [])).map((s) =>
				"state" in s ? s.state : "stopped",
			),
			p.running,
		),
	);
	let removeOpen = $state(false);

	const DOCKS = [
		{ side: "left", label: "Left dock", command: "workspace::ToggleLeftDock", icon: PanelLeft },
		{ side: "bottom", label: "Bottom dock", command: "workspace::ToggleBottomDock", icon: PanelBottom },
	] as const;

	function openInbox(id: string) {
		const n = agents.inbox.find((x) => x.id === id);
		if (!n) return;
		n.read = true;
		ws.view = "worktree";
		store.selected = n.worktreeId;
		ws.select(n.worktreeId);
		if (ws.tab(n.tabId)) ws.activate(n.tabId);
	}
</script>

<header data-tauri-drag-region class:inset={!docks.sidebar}>
	<Crumbs />
	<span class="pill t-{health.tone}" title={health.label}>
		<span class="dot" class:pulse={health.tone === "ok"}></span>{p.running ? health.label : "Stopped"}
	</span>
	<div class="grow" data-tauri-drag-region></div>
	<div class="actions">
		<NotificationBell
			notifications={agents.inbox.map((n) => ({
				id: n.id,
				title: n.title,
				body: n.body,
				read: n.read,
				time: new Date(n.ts).toLocaleTimeString(),
				variant: n.body === "Needs you" ? "warning" : "success",
			}))}
			onRead={openInbox}
			onReadAll={() => agents.inbox.forEach((n) => (n.read = true))}
			onClear={() => (agents.inbox = [])}
		/>
		<div class="docks" role="group" aria-label="Docks">
			{#each DOCKS as d (d.side)}
				<Tip label="Toggle {d.label.toLowerCase()}" command={d.command}>
					{#snippet child(props)}
						<button
							{...props}
							class="dock-btn"
							class:on={docks[d.side]}
							aria-label="Toggle {d.label.toLowerCase()}"
							aria-pressed={docks[d.side]}
							onclick={() => toggleDock(d.side)}
						>
							<d.icon size={15} />
						</button>
					{/snippet}
				</Tip>
			{/each}
			<RightToggle />
		</div>
		<div class="ops" role="group" aria-label="Activity and actions">
			<PanelOps project={p} />
		</div>
		<DropdownMenu align="end">
			{#snippet trigger()}
				<span class="more" aria-label="Project menu"><Ellipsis size={16} /></span>
			{/snippet}
			<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}>
				<Code size={14} /> Open in external editor
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
				{#if p.pinned}<PinOff size={14} /> Unpin{:else}<Pin size={14} /> Pin to top{/if}
			</DropdownMenuItem>
			<DropdownMenuItem variant="destructive" onclick={() => (removeOpen = true)}>
				<Trash2 size={14} /> Remove from Atlas
			</DropdownMenuItem>
		</DropdownMenu>
		{#if p.running}
			<Button
				variant="glass"
				size="sm"
				disabled={busy}
				onclick={() => store.run(`stack:${p.id}`, () => api.stop(p.id))}
			>
				{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<Square
							size={12}
							fill="currentColor"
						/>{/if}{/snippet}
				Stop
			</Button>
		{:else}
			<div class="split" class:has-menu={p.profiles.length > 0}>
				<Button
					size="sm"
					disabled={busy || !!p.error}
					onclick={() => store.run(`stack:${p.id}`, () => api.start(p.id))}
				>
					{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<Play
								size={12}
								fill="currentColor"
							/>{/if}{/snippet}
					Start
				</Button>
				{#if p.profiles.length}
					<DropdownMenu align="end">
						{#snippet trigger()}
							<span class="split-arrow" aria-label="Start with profile"><ChevronDown size={13} /></span>
						{/snippet}
						<DropdownMenuLabel>Start with profile</DropdownMenuLabel>
						{#each p.profiles as prof (prof)}
							<DropdownMenuItem onclick={() => store.run(`stack:${p.id}`, () => api.start(p.id, [prof]))}>
								{prof}
							</DropdownMenuItem>
						{/each}
					</DropdownMenu>
				{/if}
			</div>
		{/if}
	</div>
</header>

<Dialog
	bind:open={removeOpen}
	title="Remove {p.name}?"
	description="Only removes it from the sidebar. Files and running stacks are untouched."
	size="sm"
>
	{#snippet actions()}
		<Button variant="ghost" onclick={() => (removeOpen = false)}>Cancel</Button>
		<Button
			variant="destructive"
			onclick={() => {
				removeOpen = false;
				store.run("rm", () => api.remove(p.id));
			}}>Remove</Button
		>
	{/snippet}
</Dialog>

<style>
	/* One row, 40px: the same band as the macOS traffic lights (window y ≈ 8 + 20). The crumb
	   truncates first; the actions never wrap. */
	header {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 40px;
		flex: none;
		padding: 0 10px 0 10px;
		transition: padding-left var(--dur-slow) var(--ease);
	}
	/* Sidebar hidden: the content reaches the window's left edge, so keep clear of the traffic lights. */
	header.inset {
		padding-left: 70px;
	}
	/* Windows and Linux: the native title bar holds the window buttons. */
	:global(html.opaque-window) header.inset {
		padding-left: 10px;
	}
	header > :global(.crumbs) {
		flex: 0 1 auto;
		min-width: 0;
	}
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		flex: none;
		align-self: center;
		padding: 0 8px 0 7px;
		height: 22px;
		border-radius: 10px;
		font-size: 11.5px;
		font-weight: 550;
		color: var(--c);
		background: color-mix(in oklab, var(--c) 13%, transparent);
		--c: var(--ca-text-secondary);
		transition:
			background 200ms ease-out,
			color 200ms ease-out;
	}
	.pill.t-ok {
		--c: var(--ca-semantic-success);
	}
	.pill.t-warn {
		--c: var(--ca-semantic-warning);
	}
	.pill.t-fail {
		--c: var(--ca-semantic-error);
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--c);
	}
	.dot.pulse {
		animation: live 2.4s ease-out infinite;
	}
	@keyframes live {
		0% {
			box-shadow: 0 0 0 0 color-mix(in oklab, var(--c) 55%, transparent);
		}
		70%,
		100% {
			box-shadow: 0 0 0 5px transparent;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot.pulse {
			animation: none;
		}
	}
	.grow {
		flex: 1 1 24px;
		align-self: stretch;
		min-width: 24px;
	}
	.actions {
		flex: none;
		display: flex;
		align-items: center;
		gap: 4px;
		white-space: nowrap;
	}
	.more,
	.split-arrow {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 7px;
		color: var(--ca-text-secondary);
	}
	.more {
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.more:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.split {
		display: flex;
		align-items: stretch;
	}
	.split.has-menu :global(.ca-btn) {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
	}
	.split-arrow {
		height: 100%;
		min-height: 28px;
		width: 24px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		border-radius: 0 8px 8px 0;
		box-shadow: inset 1px 0 0 color-mix(in oklab, var(--ca-brand-contrast) 25%, transparent);
	}
	.split-arrow:hover {
		background: var(--ca-brand-fill-hover);
	}
	.docks,
	.ops {
		display: inline-flex;
		align-items: center;
		gap: 1px;
		padding: 2px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
	}
	.dock-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 24px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.dock-btn:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.dock-btn.on {
		color: var(--space-accent-text, var(--ca-brand-main));
		background: var(--ca-surface-raised);
		box-shadow:
			var(--ca-shadow-xs),
			0 0 0 0.5px var(--ca-outline-subtle);
	}
</style>
