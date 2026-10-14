<script lang="ts">
	import { colorScheme } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import {
		Bot,
		CircleAlert,
		Download,
		ExternalLink,
		FileText,
		Play,
		RotateCcw,
		Settings,
		Square,
		Terminal as TerminalIcon,
	} from "@lucide/svelte";
	import { onMount, tick } from "svelte";
	import { errText } from "#lib/api.ts";
	import { fmtMem } from "#lib/format.ts";
	import { MARK, MARK_GRADIENT, MARK_VIEW } from "#lib/onboarding/intro/mark.ts";
	import { accentOf, hueOf, onAccent } from "#lib/shell/space.ts";
	import {
		EMPTY_SNAPSHOT,
		agentsLine,
		failedServices,
		idleProjects,
		moveFocus,
		overall,
		projectHeadline,
		recommend,
		restartFailedCalls,
		runningCards,
		serviceTone,
		sparkPaths,
		trayHeight,
		upLine,
		type TrayAction,
		type TrayIcon,
		type TrayProject,
		type TraySnapshot,
	} from "#lib/tray.ts";

	const SPARK_W = 280;
	const SPARK_H = 28;

	let snap = $state<TraySnapshot>(EMPTY_SNAPSHOT);
	let now = $state(Date.now());
	let busy = $state<Record<string, boolean>>({});
	let error = $state<string | null>(null);
	let root = $state<HTMLDivElement>();
	let scroller = $state<HTMLDivElement>();
	let dark = $derived(colorScheme.resolved === "dark");

	let status = $derived(overall(snap.projects));
	let cards = $derived(runningCards(snap.projects));
	let idle = $derived(idleProjects(snap.projects));
	let rec = $derived(recommend(snap, now));
	/** A failure card already says what failed; the recommendation then only adds what it does not (update, host). */
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

	// Keyboard: ↑/↓/Home/End walk every focusable row and button in reading order; Esc closes.
	function focusables(): HTMLElement[] {
		return root ? [...root.querySelectorAll<HTMLElement>("[data-nav]:not(:disabled)")] : [];
	}
	function onKey(e: KeyboardEvent) {
		if (e.key === "Escape") return void invoke("tray_hide");
		if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
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
				now = Date.now();
				const fresh = await invoke<TraySnapshot | null>("tray_snapshot");
				if (fresh) snap = fresh;
			}),
		);
		void invoke<TraySnapshot | null>("tray_snapshot").then((s) => s && (snap = s));
		syncTheme();
		const clock = setInterval(() => (now = Date.now()), 15_000);
		window.addEventListener("keydown", onKey);
		return () => {
			offs.forEach((f) => f());
			clearInterval(clock);
			window.removeEventListener("keydown", onKey);
		};
	});

	// Resize after every change that can alter the content's height.
	$effect(() => {
		void [
			cards.length,
			idle.length,
			recShown,
			error,
			snap.host.status,
			cards.map((c) => failedServices(c).length),
		];
		void fit();
	});

	const hueOfProject = (p: TrayProject) => p.hue ?? hueOf(p.id);
</script>

{#snippet tile(i: TrayIcon)}
	<span class="tile {i.kind}" aria-hidden="true">
		{#if i.kind === "image"}<img src={i.src} alt="" />{:else}{i.text}{/if}
	</span>
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
		<div class="titles">
			<b
				>Atlas{#if snap.dev}<span class="devchip">Dev</span>{/if}</b
			>
			<span class="state {status.tone}" role="status"
				><i class="dot {status.tone}" class:live={status.tone === "fail" || status.tone === "warn"}
				></i>{status.label}</span
			>
		</div>
		<button
			class="icon"
			data-nav
			onclick={() => send({ kind: "settings" })}
			aria-label="Settings"
			title="Settings"
		>
			<Settings size={15} />
		</button>
	</header>

	{#if error}
		<div class="err" role="alert">{error}</div>
	{/if}

	<div class="scroll" bind:this={scroller}>
		{#each cards as p (p.id)}
			{@const head = projectHeadline(p)}
			{@const failed = failedServices(p)}
			{@const sp = sparkPaths(p.cpuSeries, SPARK_W, SPARK_H, 3)}
			<article
				class="card project {head.state}"
				style:--c={accentOf(p.id, dark, p.hue)}
				style:--on={onAccent(dark, hueOfProject(p))}
			>
				<div class="row">
					<button
						class="name-btn"
						data-nav
						onclick={() => send({ kind: "project", id: p.id })}
						aria-label="Open {p.name}"
					>
						{@render tile(p.icon)}<span class="name">{p.name}</span>
						{#if p.branch}<span class="branch">{p.branch}</span>{/if}
					</button>
					<span class="chip {head.state}" title={upLine(p)}
						><i class="dot {head.state === 'failed' ? 'fail' : head.state === 'starting' ? 'warn' : 'ok'}"
						></i>{head.text}</span
					>
				</div>
				<div class="bar" title={upLine(p)} aria-hidden="true">
					{#each p.services as s (s.name)}
						<i class="seg {serviceTone(s.state)}" title="{s.name}: {s.state}"></i>
					{/each}
				</div>
				<div class="stats">
					<span>{upLine(p)}</span><span>{p.cpu}% CPU</span><span>{fmtMem(p.mem)}</span>
				</div>
				{#if sp.line}
					<svg class="spark" viewBox="0 0 {SPARK_W} {SPARK_H}" preserveAspectRatio="none" aria-hidden="true">
						<path class="area" d={sp.area} />
						<path class="line" d={sp.line} vector-effect="non-scaling-stroke" />
					</svg>
				{/if}
				<div class="actions">
					{#if failed.length}
						<button
							class="btn primary"
							data-nav
							disabled={busy[`${p.id}:restart`]}
							onclick={() => restartFailed(p)}
						>
							<RotateCcw size={11} />Restart failed
						</button>
						<button
							class="btn"
							data-nav
							onclick={() => send({ kind: "logs", id: p.id, service: failed[0]! })}
						>
							<FileText size={11} />Logs
						</button>
					{/if}
					<button class="btn ghost" data-nav onclick={() => send({ kind: "project", id: p.id })}>
						<ExternalLink size={11} />Open
					</button>
					<button
						class="btn ghost end"
						data-nav
						disabled={busy[p.id]}
						onclick={() => toggleStack(p.id, true)}
						aria-label="Stop {p.name}"
					>
						<Square size={11} />Stop
					</button>
				</div>
			</article>
		{/each}

		<section class="tiles" aria-label="Agents and terminals">
			<button
				class="card tile-stat"
				data-nav
				onclick={() => send({ kind: "agents", tab: snap.agents.focusTab })}
			>
				<span class="label"><Bot size={13} />Agents</span>
				<span class="big">{snap.agents.needsYou + snap.agents.working}</span>
				<span class="meta" class:attn={snap.agents.needsYou > 0}>{agentsLine(snap.agents)}</span>
			</button>
			<button class="card tile-stat" data-nav onclick={() => send({ kind: "terminal" })}>
				<span class="label"
					><TerminalIcon size={13} />Terminals{#if snap.host.status === "outdated"}<i class="dot warn"
						></i>{/if}</span
				>
				<span class="big">{snap.terminals.live}</span>
				<span class="meta"
					>{snap.host.version ? `host ${snap.host.version}` : "host"}{#if snap.host.status === "outdated"}
						· outdated{/if}</span
				>
			</button>
		</section>

		{#if recShown}
			<section class="card rec {recShown.icon}">
				<span class="rec-icon">
					{#if recShown.icon === "update"}<Download size={16} />{:else}<CircleAlert size={16} />{/if}
				</span>
				<div class="rec-text">
					<b>{recShown.title}</b>
					<span>{recShown.detail}</span>
				</div>
				<button class="cta" data-nav onclick={() => send(recShown.action)}>{recShown.cta}</button>
			</section>
		{/if}

		{#if idle.length}
			<section class="idle" aria-label="Stopped projects">
				<h2>Stopped · {idle.length}</h2>
				{#each idle as p (p.id)}
					<div
						class="idle-row"
						style:--c={accentOf(p.id, dark, p.hue)}
						style:--on={onAccent(dark, hueOfProject(p))}
					>
						<button
							class="name-btn"
							data-nav
							onclick={() => send({ kind: "project", id: p.id })}
							aria-label="Open {p.name}"
						>
							{@render tile(p.icon)}<span class="name">{p.name}</span>
						</button>
						<button
							class="start"
							data-nav
							disabled={busy[p.id]}
							onclick={() => toggleStack(p.id, false)}
							aria-label="Start {p.name}"
						>
							<Play size={11} />Start
						</button>
					</div>
				{/each}
			</section>
		{/if}
	</div>

	<footer>
		<button class="open" data-nav onclick={() => send({ kind: "open" })}>Open Atlas</button>
	</footer>
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
		gap: 10px;
		padding: 14px 14px 10px;
	}
	.mark {
		width: 30px;
		height: 30px;
		flex: none;
	}
	.titles {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}
	.titles b {
		font-size: 14px;
		letter-spacing: -0.01em;
	}
	.devchip {
		margin-left: 6px;
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
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
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
		animation: halo 2.4s ease-out infinite;
	}
	@keyframes halo {
		0% {
			box-shadow: 0 0 0 0 color-mix(in oklab, currentColor 45%, transparent);
		}
		70%,
		100% {
			box-shadow: 0 0 0 6px transparent;
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
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.icon:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.err {
		margin: 0 14px 8px;
		padding: 8px 10px;
		border-radius: 10px;
		font-size: 12px;
		background: color-mix(in oklab, var(--fail) 16%, transparent);
		color: var(--fail);
	}
	/* The list scrolls inside the window: header and footer stay put. */
	.scroll {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 10px;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		padding: 2px 14px 10px;
		scrollbar-width: thin;
	}
	.card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 7px;
		min-width: 0;
		padding: 11px 12px;
		border: 1px solid var(--line);
		border-radius: 12px;
		background: var(--card);
		color: inherit;
		text-align: left;
		font: inherit;
		animation: rise 160ms var(--ease, ease-out) both;
	}
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(3px);
		}
	}
	/* State rail: the card's health at a glance, not just a dot. */
	.project::before {
		content: "";
		position: absolute;
		left: 0;
		top: 10px;
		bottom: 10px;
		width: 3px;
		border-radius: 0 3px 3px 0;
		background: var(--ok);
	}
	.project.failed::before {
		background: var(--fail);
	}
	.project.starting::before {
		background: var(--warn);
	}
	.project.failed {
		border-color: color-mix(in oklab, var(--fail) 38%, var(--line));
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.name-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		padding: 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		text-align: left;
	}
	.name {
		overflow: hidden;
		font-size: 12.5px;
		font-weight: 620;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.branch {
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 10px;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.chip {
		display: inline-flex;
		align-items: center;
		flex: none;
		gap: 5px;
		max-width: 52%;
		overflow: hidden;
		font-size: 11px;
		font-weight: 560;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.chip.failed {
		color: var(--fail);
	}
	.chip.starting {
		color: var(--warn);
	}
	/* One segment per service: failures read as red bars, not as one of ten similar dots. */
	.bar {
		display: flex;
		gap: 2px;
		height: 4px;
	}
	.seg {
		flex: 1;
		border-radius: 2px;
		background: color-mix(in oklab, var(--ca-text-primary) 16%, transparent);
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
	.stats {
		display: flex;
		gap: 10px;
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
	}
	/* Own SVG, not a canvas chart: it stretches to the card, the stroke keeps its width, nothing is clipped. */
	.spark {
		display: block;
		width: 100%;
		height: 28px;
		pointer-events: none;
	}
	.spark .area {
		fill: color-mix(in oklab, var(--c) 16%, transparent);
	}
	.spark .line {
		fill: none;
		stroke: var(--c);
		stroke-width: 1.5;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
	.actions {
		display: flex;
		gap: 6px;
		margin-top: 1px;
	}
	.end {
		margin-left: auto;
	}
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		border: 1px solid var(--line);
		border-radius: 8px;
		background: var(--card-hi);
		color: inherit;
		font: inherit;
		font-size: 11px;
		font-weight: 560;
		white-space: nowrap;
		transition: background 120ms ease-out;
	}
	.btn:hover {
		background: color-mix(in oklab, var(--card-hi), var(--ca-text-primary) 8%);
	}
	.btn.primary {
		border-color: transparent;
		background: var(--ca-brand-main);
		color: var(--ca-brand-contrast, #04122e);
	}
	.btn.primary:hover {
		background: color-mix(in oklab, var(--ca-brand-main), white 12%);
	}
	.btn.ghost {
		background: transparent;
		border-color: transparent;
		color: var(--ca-text-secondary);
	}
	.btn.ghost:hover {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.btn:disabled {
		opacity: 1;
		color: var(--ca-text-disabled);
	}
	.tiles {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}
	.tile-stat {
		gap: 2px;
		cursor: default;
		transition: background 120ms ease-out;
	}
	.tile-stat:hover {
		background: var(--card-hi);
	}
	.label {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 11.5px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.big {
		font-size: 24px;
		font-weight: 650;
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}
	.meta {
		overflow: hidden;
		font-size: 11px;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--ca-text-secondary);
	}
	.meta.attn {
		color: var(--warn);
		font-weight: 600;
	}
	.idle {
		display: flex;
		flex-direction: column;
	}
	.idle h2 {
		margin: 2px 2px 4px;
		font-size: 10px;
		font-weight: 650;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	.idle-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 3px 4px 3px 6px;
		border-radius: 8px;
	}
	.idle-row:hover,
	.idle-row:focus-within {
		background: var(--ca-action-hover);
	}
	.idle-row .name {
		font-weight: 500;
		color: var(--ca-text-secondary);
	}
	.idle-row:hover .name,
	.idle-row:focus-within .name {
		color: var(--ca-text-primary);
	}
	/* Hover/keyboard reveal: the Start button is there for the eye that needs it, quiet for the one that scans. */
	.start {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 9px;
		border: 1px solid var(--line);
		border-radius: 7px;
		background: var(--card-hi);
		color: inherit;
		font: inherit;
		font-size: 11px;
		opacity: 0;
		transition: opacity 120ms ease-out;
	}
	.idle-row:hover .start,
	.idle-row:focus-within .start,
	.start:focus-visible {
		opacity: 1;
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
		gap: 10px;
	}
	.rec-icon {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: 9px;
		background: color-mix(in oklab, var(--ca-brand-main) 18%, transparent);
		color: var(--ca-brand-main);
		flex: none;
	}
	.rec-text {
		display: flex;
		flex: 1;
		flex-direction: column;
		min-width: 0;
		font-size: 12px;
	}
	.rec-text span {
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
	.cta {
		padding: 5px 12px;
		border: 0;
		border-radius: 8px;
		background: var(--ca-brand-main);
		color: var(--ca-brand-contrast, #04122e);
		font: inherit;
		font-size: 12px;
		font-weight: 600;
	}
	footer {
		padding: 8px 14px 14px;
		border-top: 1px solid var(--line);
	}
	.open {
		width: 100%;
		padding: 7px 12px;
		border: 1px solid var(--line);
		border-radius: 9px;
		background: var(--card);
		color: inherit;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		transition: background 120ms ease-out;
	}
	.open:hover {
		background: var(--card-hi);
	}
	@media (prefers-reduced-motion: reduce) {
		.card,
		.dot.live {
			animation: none;
		}
		.btn,
		.start,
		.open,
		.tile-stat {
			transition: none;
		}
	}
</style>
