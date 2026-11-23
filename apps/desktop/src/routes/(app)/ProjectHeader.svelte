<script lang="ts">
	import {
		Button,
		toast,
		DropdownMenu,
		DropdownMenuItem,
		DropdownMenuLabel,
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
	import { removeProject } from "#lib/components/sidebar/projects.ts";
	import PanelOps from "#lib/panel/PanelOps.svelte";
	import Crumbs from "#lib/shell/Crumbs.svelte";
	import InboxBell from "#lib/shell/InboxBell.svelte";
	import RightToggle from "#lib/shell/RightToggle.svelte";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { docks, toggleDock } from "#lib/stores/docks.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { summarize } from "#lib/panel/metrics.ts";
	import { m } from "#lib/i18n/m.ts";

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

	const DOCKS = [
		{
			side: "left",
			label: () => m.shell_project_header_toggle_left_dock(),
			command: "workspace::ToggleLeftDock",
			icon: PanelLeft,
		},
		{
			side: "bottom",
			label: () => m.shell_project_header_toggle_bottom_dock(),
			command: "workspace::ToggleBottomDock",
			icon: PanelBottom,
		},
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

	/** Write the detected config as atlas.yaml; the project then reloads as file-backed. */
	async function saveConfig(id: string) {
		const saved = await store.run(`save:${id}`, () => api.saveConfig(id));
		if (!saved) return;
		toast.success(m.shell_project_header_saved_atlas_yaml());
		await store.refresh();
		void store.loadConfig(id);
	}

	/** A plain folder: explorer and editor only. */
	let folder = $derived(p.kind === "folder");

	/** "Use as project": the folder keeps its files; it gets the services, Changes and PR parts of the app. */
	async function promote(id: string) {
		const done = await store.run(`promote:${id}`, async () => (await api.promote(id), true));
		if (!done) return;
		await store.refresh();
		void store.loadConfig(id);
	}
</script>

<header data-tauri-drag-region class:inset={!docks.sidebar}>
	<Crumbs />
	{#if folder}
		<span class="detected" title={m.shell_project_header_a_plain_folder_atlas_shows()}
			>{m.shell_project_header_folder()}</span
		>
	{:else}
		<span class="pill t-{health.tone}" title={health.label}>
			<span class="dot" class:pulse={health.tone === "ok"}></span>{health.label}
		</span>
	{/if}
	{#if !folder && p.config_source === "detected"}
		<span class="detected" title={m.shell_project_header_no_atlas_yaml_in_this()}
			>{m.shell_project_header_auto_detected()}</span
		>
		<Button
			variant="ghost"
			size="sm"
			disabled={!!store.busy[`save:${p.id}`]}
			onclick={() => saveConfig(p.id)}
		>
			{m.shell_project_header_save_as_atlas_yaml()}
		</Button>
	{/if}
	<div class="grow" data-tauri-drag-region></div>
	<div class="actions">
		<InboxBell
			items={agents.inbox.map((n) => ({
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
			onSettings={() => updates.show("notifications")}
		/>
		<div class="docks" role="group" aria-label={m.shell_project_header_docks()}>
			{#each DOCKS as d (d.side)}
				<Tip label={d.label()} command={d.command}>
					{#snippet child(props)}
						<button
							{...props}
							class="dock-btn"
							class:on={docks[d.side]}
							aria-label={d.label()}
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
		{#if !folder}
			<div class="ops" role="group" aria-label={m.shell_project_header_activity_and_actions()}>
				<PanelOps project={p} />
			</div>
		{/if}
		<DropdownMenu align="end">
			{#snippet triggerChild(props)}
				<button type="button" {...props} class="more" aria-label={m.shell_project_header_project_menu()}
					><Ellipsis size={16} /></button
				>
			{/snippet}
			<DropdownMenuItem onclick={() => store.run("ed", () => api.openEditor(p.id))}>
				<Code size={14} />
				{m.shell_project_header_open_in_external_editor()}
			</DropdownMenuItem>
			<DropdownMenuItem onclick={() => store.run("pin", () => api.pin(p.id, !p.pinned))}>
				{#if p.pinned}<PinOff size={14} /> {m.shell_project_header_unpin()}{:else}<Pin size={14} />
					{m.shell_project_header_pin_to_top()}{/if}
			</DropdownMenuItem>
			{#if !p.parent}
				<DropdownMenuItem variant="destructive" onclick={() => removeProject(p.id)}>
					<Trash2 size={14} />
					{m.shell_project_header_remove_from_atlas()}
				</DropdownMenuItem>
			{/if}
		</DropdownMenu>
		{#if folder}
			<Button size="sm" disabled={!!store.busy[`promote:${p.id}`]} onclick={() => promote(p.id)}>
				{m.shell_project_header_use_as_project()}
			</Button>
		{:else if p.running}
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
				{m.shell_project_header_stop()}
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
					{m.shell_project_header_start()}
				</Button>
				{#if p.profiles.length}
					<DropdownMenu align="end">
						{#snippet triggerChild(props)}
							<button
								type="button"
								{...props}
								class="split-arrow"
								aria-label={m.shell_project_header_start_with_profile()}><ChevronDown size={13} /></button
							>
						{/snippet}
						<DropdownMenuLabel>{m.shell_project_header_start_with_profile()}</DropdownMenuLabel>
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

<style>
	/* One row, --title-row tall (app.css): the same band as the macOS traffic lights, whose centre is that row's
	   (trafficLightPosition in tauri.conf.json, pinned by lib/trafficLights.test.ts). The crumb
	   truncates first; the actions never wrap. */
	header {
		display: flex;
		align-items: center;
		gap: 8px;
		height: var(--title-row);
		flex: none;
		padding: 0 10px 0 10px;
		transition: padding-left var(--dur-slow) var(--ease);
	}
	/* Sidebar hidden: the content reaches the window's left edge, so keep clear of the traffic lights. */
	header.inset {
		padding-left: 70px;
	}
	/* Windows and Linux: no traffic lights to clear on the left. */
	:global(html.opaque-window) header.inset {
		padding-left: 10px;
	}
	/* Windows and floating Linux: the caption buttons sit over the row's right end (WindowControls.svelte). The band
	   only widens the row's own padding: a tiling WM has no buttons (--caption-controls 0px) and keeps its 10px (#739). */
	:global(html.custom-chrome) header {
		padding-right: max(10px, var(--caption-controls));
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
	/* Quiet marker: the stack comes from detection, not from a file (the button next to it saves one). */
	.detected {
		flex: none;
		padding: 1px 7px;
		border-radius: 10px;
		font-size: 11px;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
		background: color-mix(in oklab, var(--ca-text-secondary) 10%, transparent);
	}
	.dot {
		position: relative;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--c);
	}
	/* The expanding halo is a pseudo-element animated with transform + opacity (a box-shadow would repaint each frame). */
	.dot.pulse::after {
		content: "";
		position: absolute;
		inset: 0;
		border-radius: 50%;
		background: color-mix(in oklab, var(--c) 55%, transparent);
		animation: live 2.4s ease-out infinite;
	}
	@keyframes live {
		0% {
			transform: scale(1);
			opacity: 1;
		}
		70%,
		100% {
			transform: scale(2.8);
			opacity: 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot.pulse::after {
			animation: none;
			opacity: 0;
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
		border: 0;
		background: none;
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
