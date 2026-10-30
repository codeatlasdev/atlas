<script lang="ts">
	import { useNow } from "#lib/clock.svelte.ts";
	import { colorScheme } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import {
		Bot,
		ChevronRight,
		CircleAlert,
		Download,
		ExternalLink,
		FileText,
		PanelBottom,
		Play,
		Plus,
		RotateCcw,
		Search,
		Settings,
		Square,
	} from "@lucide/svelte";
	import { onMount, tick } from "svelte";
	import { errText } from "#lib/api.ts";
	import { MARK, MARK_GRADIENT, MARK_VIEW } from "#lib/onboarding/intro/mark.ts";
	import { accentOf, hueOf, onAccent } from "#lib/shell/space.ts";
	import {
		EMPTY_SNAPSHOT,
		agentDoing,
		agentSections,
		agentWhere,
		failedServices,
		groupProjects,
		headerSummary,
		moveFocus,
		projectHeadline,
		projectSections,
		recommend,
		restartFailedCalls,
		runningCards,
		runningMatches,
		searchOpensStopped,
		serviceTone,
		showSearch,
		trayHeight,
		upLine,
		type TrayAction,
		type TrayAgent,
		type TrayIcon,
		type TrayProject,
		type TraySnapshot,
	} from "#lib/tray.ts";

	let snap = $state<TraySnapshot>(EMPTY_SNAPSHOT);
	const clk = useNow(15_000);
	let busy = $state<Record<string, boolean>>({});
	let error = $state<string | null>(null);
	let root = $state<HTMLDivElement>();
	let scroller = $state<HTMLDivElement>();
	let dark = $derived(colorScheme.resolved === "dark");
	let query = $state("");
	let stoppedOpen = $state(false);
	/** The agent dock's switch (Settings → Agents → Agent dock); null until Rust answers. */
	let dock = $state<{ enabled: boolean } | null>(null);

	let summary = $derived(headerSummary(snap));
	let sections = $derived(agentSections(snap.agents.list));
	let allStacks = $derived(runningCards(snap.projects));
	let stacks = $derived(runningMatches(allStacks, query));
	let groups = $derived(groupProjects(snap.projects));
	let lists = $derived(projectSections(groups, query));
	let searching = $derived(searchOpensStopped(query));
	let stoppedShown = $derived(stoppedOpen || searching);
	let rec = $derived(recommend(snap, clk.now));
	/** A failed stack already says what failed; the recommendation then only adds what it does not (update, host). */
	let recShown = $derived(rec && rec.icon !== "fail" ? rec : null);

	const send = (action: TrayAction) => void invoke("tray_action", { action });

	async function guarded(key: string, run: () => Promise<unknown>) {
		busy[key] = true;
		error = null;
		try {
			await run();
		} catch (e) {
			error = errText(e);
		} finally {
			busy[key] = false;
		}
	}

	const toggleStack = (id: string, running: boolean) =>
		guarded(id, () =>
			invoke(running ? "stack_stop" : "stack_start", running ? { id } : { id, services: [], profiles: [] }),
		);

	/** Restarts exactly the failed services (the same `service_cmd` the main window uses), not the whole stack. */
	const restartFailed = (p: TrayProject) =>
		guarded(`${p.id}:restart`, () => Promise.all(restartFailedCalls(p).map((c) => invoke("service_cmd", c))));

	const loadDock = () =>
		void invoke<{ enabled: boolean }>("dock_prefs")
			.then((p) => (dock = p))
			.catch(() => (dock = null));

	/** The dock command keeps the rest of its prefs (summon key, toggle key): write back what was read. */
	const toggleDock = () =>
		dock &&
		guarded("dock", async () => {
			const prefs = await invoke<{ enabled: boolean }>("dock_prefs");
			dock = await invoke("dock_prefs_set", { prefs: { ...prefs, enabled: !prefs.enabled } });
		});

	/** Theme and vibrancy follow the choice made in the main window (shared localStorage). */
	function syncTheme() {
		const pref = JSON.parse(localStorage.getItem("ca-scheme") ?? '"system"');
		colorScheme.setPreference(pref);
		void getCurrentWindow()
			.setTheme(pref === "system" ? null : pref)
			.catch(() => {});
	}

	/** The window fits its content, up to a cap (and the room the screen leaves); past it the list scrolls. */
	async function fit() {
		await tick();
		if (root) void invoke("tray_resize", { height: trayHeight(root.scrollHeight) });
	}

	// Keyboard: ↑/↓/Home/End walk every focusable row and button in reading order; Esc clears the search, then closes.
	function focusables(): HTMLElement[] {
		return root ? [...root.querySelectorAll<HTMLElement>("[data-nav]:not(:disabled)")] : [];
	}
	function onKey(e: KeyboardEvent) {
		if (e.key === "Escape") {
			if (query) return void (query = "");
			return void invoke("tray_hide");
		}
		if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
		// Home/End belong to the search field's caret while it has the focus.
		const typing = document.activeElement instanceof HTMLInputElement;
		if (typing && (e.key === "Home" || e.key === "End")) return;
		const items = focusables();
		const at = items.indexOf(document.activeElement as HTMLElement);
		const next = moveFocus(items.length, at, e.key);
		if (next >= 0 && next !== at) {
			e.preventDefault();
			items[next]!.focus();
			items[next]!.scrollIntoView({ block: "nearest" });
		}
	}

	onMount(() => {
		const offs: Array<() => void> = [];
		const keep = (p: Promise<() => void>) => void p.then((f) => offs.push(f));
		keep(listen<TraySnapshot>("tray://snapshot", (e) => (snap = e.payload)));
		keep(
			listen("tray://shown", async () => {
				syncTheme();
				clk.refresh();
				loadDock();
				const fresh = await invoke<TraySnapshot | null>("tray_snapshot");
				if (fresh) snap = fresh;
			}),
		);
		keep(listen<{ enabled: boolean }>("dock://prefs", (e) => (dock = e.payload)));
		void invoke<TraySnapshot | null>("tray_snapshot").then((s) => s && (snap = s));
		syncTheme();
		loadDock();
		window.addEventListener("keydown", onKey);
		return () => {
			offs.forEach((f) => f());
			window.removeEventListener("keydown", onKey);
		};
	});

	// Resize after every change that can alter the content's height.
	$effect(() => {
		void [
			sections.needsYou.length,
			sections.working.length,
			stacks.length,
			lists.running.length,
			lists.stoppedCount,
			stoppedShown,
			recShown,
			error,
			snap.host.status,
			stacks.map((c) => failedServices(c).length),
		];
		void fit();
	});

	const hueOfProject = (p: TrayProject) => p.hue ?? hueOf(p.id);
	const tone = (p: TrayProject) => {
		const h = projectHeadline(p).state;
		return h === "failed" ? "fail" : h === "starting" ? "warn" : p.running ? "ok" : "off";
	};
</script>

{#snippet tile(i: TrayIcon)}
	<span class="tile {i.kind}" aria-hidden="true">
		{#if i.kind === "image"}<img src={i.src} alt="" />{:else}{i.text}{/if}
	</span>
{/snippet}

{#snippet agentRow(a: TrayAgent, urgent: boolean)}
	<li class="row agent" class:urgent>
		<button
			class="main"
			data-nav
			onclick={() => send({ kind: "agents", tab: a.tabId })}
			aria-label="Jump to {a.agent} in {agentWhere(a)}"
		>
			<span class="glyph {a.state}" aria-hidden="true"><Bot size={13} /></span>
			<span class="lines">
				<span class="name">{agentWhere(a)}<span class="kind">{a.agent}</span></span>
				<span class="sub">{agentDoing(a)}</span>
			</span>
		</button>
		<span class="side">
			{#if urgent}<span class="verb">{a.state === "blocked" ? "Approval" : "Reply"}</span>{/if}
			<ChevronRight size={13} />
		</span>
	</li>
{/snippet}

{#snippet project(p: TrayProject, label: string | null, nested: boolean, title: string = p.name)}
	<li
		class="row proj"
		class:nested
		style:--c={accentOf(p.id, dark, p.hue)}
		style:--on={onAccent(dark, hueOfProject(p))}
	>
		<button
			class="main"
			data-nav
			onclick={() => send({ kind: "project", id: p.id })}
			aria-label="Open {label ? `${p.name}, ${label}` : title}"
		>
			{#if nested}<i class="dot {tone(p)}" aria-hidden="true"></i>{:else}{@render tile(p.icon)}{/if}
			<span class="name"
				>{label ?? title}{#if !nested && p.running}<i class="dot {tone(p)}"></i>{/if}</span
			>
			{#if !nested && p.branch}<span class="kind">{p.branch}</span>{/if}
		</button>
		<span class="side acts">
			<button
				class="mini start"
				data-nav
				disabled={busy[p.id]}
				onclick={() => toggleStack(p.id, p.running)}
				aria-label="{p.running ? 'Stop' : 'Start'} {label ?? title}"
				title={p.running ? "Stop" : "Start"}
			>
				{#if p.running}<Square size={12} />{:else}<Play size={12} />{/if}
			</button>
		</span>
	</li>
{/snippet}

{#snippet group(g: (typeof lists.running)[number])}
	{#if g.repoShown !== false}
		{@render project(g.repo, null, false, g.repoLabel)}
	{:else}
		<li
			class="row crumb"
			aria-hidden="true"
			style:--c={accentOf(g.repo.id, dark, g.repo.hue)}
			style:--on={onAccent(dark, hueOfProject(g.repo))}
		>
			<span class="name">{@render tile(g.repo.icon)}{g.repoLabel}</span>
		</li>
	{/if}
	{#each g.worktrees as w (w.id)}{@render project(w, w.label, true)}{/each}
{/snippet}

<div class="pop" bind:this={root}>
	<header>
		<svg class="mark" viewBox="0 0 {MARK_VIEW.w} {MARK_VIEW.h}" aria-hidden="true">
			<defs>
				<linearGradient
					id="mk"
					x1={MARK_GRADIENT.x1}
					y1={MARK_GRADIENT.y1}
					x2={MARK_GRADIENT.x2}
					y2={MARK_GRADIENT.y2}
					gradientUnits="userSpaceOnUse"
				>
					{#each MARK_GRADIENT.stops as s (s.offset)}<stop offset={s.offset} stop-color={s.color} />{/each}
				</linearGradient>
			</defs>
			<g fill="url(#mk)"
				>{#each MARK as t (t.id)}<path d={t.d} />{/each}</g
			>
		</svg>
		<span class="state {summary.tone}" role="status" title={summary.text}
			><i class="dot {summary.tone}" class:live={summary.tone === "fail" || summary.tone === "warn"}></i><span
				class="text">{summary.text}</span
			></span
		>
		<div class="tools">
			<button
				class="icon"
				data-nav
				onclick={() => send({ kind: "new-agent" })}
				aria-label="New agent"
				title="New agent"
			>
				<Plus size={15} />
			</button>
			{#if dock}
				<button
					class="icon"
					class:on={dock.enabled}
					data-nav
					disabled={busy.dock}
					aria-pressed={dock.enabled}
					onclick={toggleDock}
					aria-label="Agent dock"
					title={dock.enabled ? "Hide the agent dock" : "Show the agent dock"}
				>
					<PanelBottom size={15} />
				</button>
			{/if}
			<button
				class="icon"
				data-nav
				onclick={() => send({ kind: "settings" })}
				aria-label="Settings"
				title="Settings"
			>
				<Settings size={15} />
			</button>
			<button
				class="icon open"
				data-nav
				onclick={() => send({ kind: "open" })}
				aria-label="Open Atlas"
				title="Open Atlas"
			>
				<ExternalLink size={15} />
			</button>
		</div>
	</header>

	{#if error}
		<div class="err" role="alert">{error}</div>
	{/if}

	<div class="scroll" bind:this={scroller}>
		{#if sections.needsYou.length}
			<section aria-label="Needs you">
				<h2 class="attn">Needs you</h2>
				<ul>
					{#each sections.needsYou as a (a.tabId)}{@render agentRow(a, true)}{/each}
				</ul>
			</section>
		{/if}

		{#if recShown}
			<section class="rec {recShown.icon}" aria-label="Recommendation">
				<span class="rec-icon">
					{#if recShown.icon === "update"}<Download size={14} />{:else}<CircleAlert size={14} />{/if}
				</span>
				<span class="lines">
					<span class="name">{recShown.title}</span>
					<span class="sub">{recShown.detail}</span>
				</span>
				<button class="cta" data-nav onclick={() => send(recShown.action)}>{recShown.cta}</button>
			</section>
		{/if}

		{#if sections.working.length}
			<section aria-label="Working">
				<h2>Working</h2>
				<ul>
					{#each sections.working as a (a.tabId)}{@render agentRow(a, false)}{/each}
				</ul>
			</section>
		{/if}

		{#if stacks.length}
			<section aria-label="Running stacks">
				<h2>Running</h2>
				<ul>
					{#each stacks as p (p.id)}
						{@const head = projectHeadline(p)}
						{@const failed = failedServices(p)}
						<li
							class="row stack {head.state}"
							style:--c={accentOf(p.id, dark, p.hue)}
							style:--on={onAccent(dark, hueOfProject(p))}
						>
							<button
								class="main"
								data-nav
								onclick={() => send({ kind: "project", id: p.id })}
								aria-label="Open {p.name}{p.branch ? `, ${p.branch}` : ''}"
							>
								{@render tile(p.icon)}
								<span class="lines">
									<span class="name"
										>{p.name}{#if p.branch}<span class="kind">{p.branch}</span>{/if}</span
									>
									<span class="sub {head.state}" title={upLine(p)}>
										<span class="bar" aria-hidden="true"
											>{#each p.services as s (s.name)}<i
													class="seg {serviceTone(s.state)}"
													title="{s.name}: {s.state}"
												></i>{/each}</span
										>{head.text}
									</span>
								</span>
							</button>
							<span class="side acts">
								{#if failed.length}
									<button
										class="mini"
										data-nav
										disabled={busy[`${p.id}:restart`]}
										onclick={() => restartFailed(p)}
										aria-label="Restart failed services of {p.name}"
										title="Restart failed"><RotateCcw size={12} /></button
									>
									<button
										class="mini"
										data-nav
										onclick={() => send({ kind: "logs", id: p.id, service: failed[0]! })}
										aria-label="Logs of {failed[0]}"
										title="Logs"><FileText size={12} /></button
									>
								{/if}
								<button
									class="mini"
									data-nav
									disabled={busy[p.id]}
									onclick={() => toggleStack(p.id, true)}
									aria-label="Stop {p.name}"
									title="Stop"><Square size={12} /></button
								>
							</span>
						</li>
					{/each}
				</ul>
			</section>
		{/if}

		{#if showSearch(snap.projects.length) || query}
			<label class="search">
				<Search size={13} aria-hidden="true" />
				<input
					type="search"
					placeholder="Find a project or branch"
					aria-label="Find a project or branch"
					bind:value={query}
					autocomplete="off"
					spellcheck="false"
				/>
			</label>
		{/if}

		{#if lists.running.length}
			<section aria-label="Projects">
				<h2>Projects</h2>
				<ul>
					{#each lists.running as g (g.repo.id)}{@render group(g)}{/each}
				</ul>
			</section>
		{/if}

		{#if lists.stopped.length}
			<section aria-label="Stopped projects">
				<button
					class="disclose"
					data-nav
					aria-expanded={stoppedShown}
					onclick={() => (stoppedOpen = !stoppedOpen)}
				>
					<ChevronRight size={12} class={stoppedShown ? "turned" : ""} />Stopped ({lists.stoppedCount})
				</button>
				{#if stoppedShown}
					<ul>
						{#each lists.stopped as g (g.repo.id)}{@render group(g)}{/each}
					</ul>
				{/if}
			</section>
		{:else if query && !stacks.length}
			<p class="empty">No project matches “{query.trim()}”.</p>
		{/if}
	</div>

	{#if snap.host.version || snap.dev}
		<footer>
			{#if snap.dev}<span class="devchip" title="Dev build: updates are paused">Dev</span>{/if}
			{#if snap.host.version}<button
					class="ver"
					data-nav
					onclick={() => send({ kind: "host" })}
					title="Terminal host"
				>
					host {snap.host.version}{#if snap.host.status === "outdated"}<i class="dot warn"></i>outdated{/if}
					· {snap.terminals.live}
					{snap.terminals.live === 1 ? "terminal" : "terminals"}
				</button>{/if}
		</footer>
	{/if}
</div>

<style>
	/* The page owns the ground (the theme's chrome surface, theme/ground.ts): never the OS material's brightness, which
	   is the wallpaper's. The window stays transparent only for its rounded corners and shadow. Layers climb the theme's
	   surface ladder: sunken (ground) < base < raised < overlay. */
	:global(html),
	:global(body) {
		background: transparent !important;
	}
	.pop {
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		height: 100vh;
		max-height: 100vh;
		overflow: hidden;
		border-radius: 14px;
		color: var(--ca-text-primary);
		background:
			radial-gradient(
				120% 70% at 50% -10%,
				color-mix(in oklab, var(--ca-brand-main) 10%, transparent),
				transparent 70%
			),
			var(--ca-surface-sunken);
		box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--ca-text-primary) 9%, transparent);
		--card: var(--ca-surface-raised);
		--card-hi: var(--ca-surface-overlay);
		--line: color-mix(in oklab, var(--ca-text-primary) 10%, transparent);
		--ok: var(--ca-semantic-success);
		--warn: var(--ca-semantic-warning);
		--fail: var(--ca-semantic-error);
	}
	header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 8px 8px 12px;
	}
	.mark {
		width: 20px;
		height: 20px;
		flex: none;
	}
	.tools {
		display: flex;
		gap: 1px;
		flex: none;
	}
	.devchip {
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 9.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 14%, transparent);
	}
	.state {
		display: inline-flex;
		align-items: center;
		flex: 1;
		gap: 6px;
		min-width: 0;
		font-size: 12.5px;
		font-weight: 560;
		color: var(--ca-text-secondary);
	}
	.state.ok {
		color: var(--ca-text-primary);
	}
	.state .text {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state.fail {
		color: var(--fail);
	}
	.state.warn {
		color: var(--warn);
	}
	.dot {
		display: inline-block;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-text-disabled);
		flex: none;
	}
	.dot.ok {
		background: var(--ok);
	}
	.dot.warn {
		background: var(--warn);
	}
	.dot.fail {
		background: var(--fail);
	}
	/* The only looping motion: a slow halo on an unhealthy dot. Off for reduced motion. */
	.dot.live {
		position: relative;
	}
	.dot.live::after {
		content: "";
		position: absolute;
		inset: 0;
		border-radius: 50%;
		background: color-mix(in oklab, currentColor 45%, transparent);
		animation: halo 2.4s ease-out infinite;
	}
	@keyframes halo {
		0% {
			transform: scale(1);
			opacity: 1;
		}
		70%,
		100% {
			transform: scale(3);
			opacity: 0;
		}
	}
	.state.fail .dot.live {
		color: var(--fail);
	}
	.state.warn .dot.live {
		color: var(--warn);
	}
	.icon {
		display: grid;
		place-items: center;
		width: 26px;
		height: 26px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.icon:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.icon.on {
		color: var(--ca-brand-main);
	}
	.icon:disabled {
		color: var(--ca-text-disabled);
	}
	.err {
		margin: 0 10px 6px;
		padding: 7px 9px;
		border-radius: 8px;
		font-size: 12px;
		background: color-mix(in oklab, var(--fail) 16%, transparent);
		color: var(--fail);
	}
	/* The list scrolls inside the window: header and footer stay put. */
	.scroll {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 6px;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: 0 6px 8px;
		scrollbar-width: thin;
	}
	section {
		display: flex;
		flex-direction: column;
	}
	h2 {
		margin: 6px 8px 2px;
		font-size: 11px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	h2.attn {
		color: var(--warn);
	}
	ul {
		display: flex;
		flex-direction: column;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.row {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		min-width: 0;
		border-radius: 8px;
	}
	.row:hover,
	.row:focus-within {
		background: var(--ca-action-hover);
	}
	.main {
		display: flex;
		align-items: center;
		flex: 1;
		gap: 8px;
		min-width: 0;
		padding: 5px 4px 5px 8px;
		border: 0;
		border-radius: 8px;
		background: none;
		color: inherit;
		font: inherit;
		text-align: left;
	}
	.lines {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.name {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		overflow: hidden;
		font-size: 12.5px;
		font-weight: 560;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.kind {
		overflow: hidden;
		max-width: 11em;
		padding: 0 6px;
		border-radius: 999px;
		font-size: 10.5px;
		font-weight: 450;
		text-overflow: ellipsis;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.sub {
		display: flex;
		align-items: center;
		gap: 6px;
		overflow: hidden;
		font-size: 11px;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.sub.failed {
		color: var(--fail);
	}
	.sub.starting {
		color: var(--warn);
	}
	.side {
		display: flex;
		align-items: center;
		flex: none;
		gap: 4px;
		padding-right: 6px;
		color: var(--ca-text-disabled);
	}
	.verb {
		font-size: 11px;
		font-weight: 600;
		color: var(--warn);
	}
	.agent.urgent {
		background: color-mix(in oklab, var(--warn) 9%, transparent);
	}
	.agent.urgent:hover,
	.agent.urgent:focus-within {
		background: color-mix(in oklab, var(--warn) 16%, transparent);
	}
	.glyph {
		display: grid;
		place-items: center;
		flex: none;
		width: 18px;
		height: 18px;
		border-radius: 5px;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.glyph.working {
		color: var(--ca-brand-main);
		background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
	}
	.glyph.waiting,
	.glyph.blocked {
		color: var(--warn);
		background: color-mix(in oklab, var(--warn) 18%, transparent);
	}
	/* Per-service segments: failures read as red bars, not as one of ten similar dots. */
	.bar {
		display: inline-flex;
		flex: none;
		gap: 1px;
		width: 28px;
		height: 3px;
	}
	.seg {
		flex: 1;
		border-radius: 1px;
		background: color-mix(in oklab, var(--ca-text-primary) 18%, transparent);
	}
	.seg.ok {
		background: var(--ok);
	}
	.seg.warn {
		background: var(--warn);
	}
	.seg.fail {
		background: var(--fail);
	}
	.stack.failed .name {
		color: var(--fail);
	}
	/* Row actions appear on hover or keyboard focus; a failed stack keeps its buttons visible. */
	.acts {
		opacity: 0;
		transition: opacity 120ms ease-out;
	}
	.row:hover .acts,
	.row:focus-within .acts,
	.stack.failed .acts {
		opacity: 1;
	}
	.mini {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.mini:hover {
		background: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
		color: var(--ca-text-primary);
	}
	.mini:disabled {
		color: var(--ca-text-disabled);
	}
	.crumb {
		padding: 5px 8px;
		color: var(--ca-text-secondary);
	}
	.crumb .name {
		font-weight: 450;
	}
	.proj.nested .main {
		padding-left: 30px;
	}
	.proj.nested .name {
		font-weight: 450;
		color: var(--ca-text-secondary);
	}
	.proj.nested:hover .name,
	.proj.nested:focus-within .name {
		color: var(--ca-text-primary);
	}
	.search {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 4px 2px 2px;
		padding: 0 8px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--card);
		color: var(--ca-text-secondary);
	}
	.search:focus-within {
		border-color: var(--ca-brand-main);
	}
	.search input {
		flex: 1;
		min-width: 0;
		padding: 5px 0;
		border: 0;
		outline: 0;
		background: none;
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12px;
	}
	.empty {
		margin: 6px 8px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.disclose {
		display: flex;
		align-items: center;
		gap: 4px;
		margin: 2px 0;
		padding: 4px 8px;
		border: 0;
		border-radius: 8px;
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 11px;
		font-weight: 600;
		text-align: left;
	}
	.disclose:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.disclose :global(svg) {
		transition: transform 120ms ease-out;
	}
	.disclose :global(svg.turned) {
		transform: rotate(90deg);
	}
	.tile {
		display: inline-grid;
		place-items: center;
		flex: none;
		width: 18px;
		height: 18px;
		border-radius: 5px;
		overflow: hidden;
		font-size: 10px;
		font-weight: 700;
		line-height: 1;
		color: var(--on, #000);
		background: linear-gradient(160deg, color-mix(in oklab, var(--c) 82%, white), var(--c));
	}
	.tile.emoji {
		font-size: 12px;
		background: color-mix(in oklab, var(--c) 16%, transparent);
	}
	.tile.image {
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.tile img {
		width: 80%;
		height: 80%;
		object-fit: contain;
	}
	.rec {
		flex-direction: row;
		align-items: center;
		gap: 8px;
		padding: 6px 6px 6px 8px;
		border-radius: 8px;
		background: color-mix(in oklab, var(--ca-brand-main) 10%, transparent);
	}
	.rec.fail {
		background: color-mix(in oklab, var(--fail) 10%, transparent);
	}
	.rec-icon {
		display: grid;
		place-items: center;
		flex: none;
		width: 18px;
		height: 18px;
		color: var(--ca-brand-main);
	}
	.rec .lines {
		flex: 1;
	}
	.cta {
		flex: none;
		padding: 3px 10px;
		border: 0;
		border-radius: 7px;
		background: var(--ca-brand-main);
		color: var(--ca-brand-contrast, #04122e);
		font: inherit;
		font-size: 11.5px;
		font-weight: 600;
	}
	footer {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 10px 8px;
		border-top: 1px solid var(--line);
	}
	.ver {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 4px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 10.5px;
		font-variant-numeric: tabular-nums;
	}
	.ver:hover {
		color: var(--ca-text-primary);
	}
	button:focus-visible {
		outline: 2px solid var(--ca-brand-main);
		outline-offset: -2px;
	}
	@media (prefers-reduced-motion: reduce) {
		.dot.live::after {
			animation: none;
		}
		.acts,
		.disclose :global(svg) {
			transition: none;
		}
	}
</style>
