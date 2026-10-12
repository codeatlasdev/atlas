<script lang="ts">
	import { Sparkline } from "@codeatlasdev/ui/charts";
	import { colorScheme } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import {
		Bot,
		CircleAlert,
		Download,
		ExternalLink,
		Play,
		Settings,
		Square,
		Terminal as TerminalIcon,
	} from "@lucide/svelte";
	import { onMount, tick } from "svelte";
	import { errText } from "#lib/api.ts";
	import { fmtMem } from "#lib/format.ts";
	import { accentOf } from "#lib/shell/space.ts";
	import {
		EMPTY_SNAPSHOT,
		agentsLine,
		idleProjects,
		overall,
		projectTone,
		recommend,
		runningCards,
		trayHeight,
		upLine,
		type TrayAction,
		type TrayIcon,
		type TraySnapshot,
	} from "#lib/tray.ts";

	let snap = $state<TraySnapshot>(EMPTY_SNAPSHOT);
	let now = $state(Date.now());
	let busy = $state<Record<string, boolean>>({});
	let error = $state<string | null>(null);
	let root = $state<HTMLDivElement>();
	let dark = $derived(colorScheme.resolved === "dark");

	let status = $derived(overall(snap.projects));
	let cards = $derived(runningCards(snap.projects));
	let idle = $derived(idleProjects(snap.projects));
	let rec = $derived(recommend(snap, now));

	const send = (action: TrayAction) => void invoke("tray_action", { action });

	async function toggleStack(id: string, running: boolean) {
		busy[id] = true;
		error = null;
		try {
			await invoke(
				running ? "stack_stop" : "stack_start",
				running ? { id } : { id, services: [], profiles: [] },
			);
		} catch (e) {
			error = errText(e);
		} finally {
			busy[id] = false;
		}
	}

	/** Theme and vibrancy follow the choice made in the main window (shared localStorage). */
	function syncTheme() {
		const pref = JSON.parse(localStorage.getItem("ca-scheme") ?? '"system"');
		colorScheme.setPreference(pref);
		void getCurrentWindow()
			.setTheme(pref === "system" ? null : pref)
			.catch(() => {});
	}

	/** The window fits its content, up to a cap; past it the list scrolls. */
	async function fit() {
		await tick();
		if (root) void invoke("tray_resize", { height: trayHeight(root.scrollHeight) });
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
		const tick1 = setInterval(() => (now = Date.now()), 15_000);
		const esc = (e: KeyboardEvent) => e.key === "Escape" && void invoke("tray_hide");
		window.addEventListener("keydown", esc);
		return () => {
			offs.forEach((f) => f());
			clearInterval(tick1);
			window.removeEventListener("keydown", esc);
		};
	});

	// Resize after every change that can alter the content's height.
	$effect(() => {
		void [cards.length, idle.length, rec, error, snap.host.status];
		void fit();
	});
</script>

{#snippet tile(i: TrayIcon)}
	<span class="tile {i.kind}" aria-hidden="true">
		{#if i.kind === "image"}<img src={i.src} alt="" />{:else}{i.text}{/if}
	</span>
{/snippet}

<div class="pop" class:dark bind:this={root}>
	<header>
		<span class="mark" aria-hidden="true"></span>
		<div class="titles">
			<b
				>Atlas{#if snap.dev}<span class="devchip">Dev</span>{/if}</b
			>
			<span class="state {status.tone}"><i class="dot {status.tone}"></i>{status.label}</span>
		</div>
	</header>

	{#if error}
		<div class="err" role="alert">{error}</div>
	{/if}

	<section class="grid">
		{#each cards as p (p.id)}
			{@const tone = projectTone(p)}
			<article class="card project" style:--c={accentOf(p.id, dark, p.hue)}>
				<button class="hit" onclick={() => send({ kind: "project", id: p.id })} aria-label="Open {p.name}"
				></button>
				<div class="row">
					<span class="name">{@render tile(p.icon)}{p.name}</span>
					<i class="dot {tone}"></i>
				</div>
				<div class="dots" title={upLine(p)}>
					{#each p.services as s (s.name)}
						<i
							class="dot {s.state === 'running'
								? 'ok'
								: s.state === 'failed'
									? 'fail'
									: s.state === 'starting'
										? 'warn'
										: 'off'}"
							title="{s.name}: {s.state}"
						></i>
					{/each}
				</div>
				<div class="meta">{upLine(p)} · {p.cpu}% · {fmtMem(p.mem)}</div>
				<div class="spark">
					<Sparkline data={p.cpuSeries} height="22px" color="var(--c)" />
				</div>
				<div class="actions">
					<button class="btn" disabled={busy[p.id]} onclick={() => toggleStack(p.id, true)}>
						<Square size={11} />Stop
					</button>
					<button class="btn ghost" onclick={() => send({ kind: "project", id: p.id })}>
						<ExternalLink size={11} />Open
					</button>
				</div>
			</article>
		{/each}

		<button class="card agents" onclick={() => send({ kind: "agents", tab: snap.agents.focusTab })}>
			<div class="row"><span class="name"><Bot size={13} /> Agents</span></div>
			<div class="big">{snap.agents.needsYou + snap.agents.working}</div>
			<div class="meta" class:attn={snap.agents.needsYou > 0}>{agentsLine(snap.agents)}</div>
		</button>

		<button class="card host" onclick={() => send({ kind: "terminal" })}>
			<div class="row">
				<span class="name"><TerminalIcon size={13} /> Terminals</span>
				{#if snap.host.status === "outdated"}<i class="dot warn"></i>{/if}
			</div>
			<div class="big">{snap.terminals.live}</div>
			<div class="meta">
				{snap.host.version ? `host ${snap.host.version}` : "host"}
				{#if snap.host.status === "outdated"}· outdated{/if}
			</div>
		</button>
	</section>

	{#if idle.length}
		<section class="idle">
			{#each idle as p (p.id)}
				<div class="idle-row" style:--c={accentOf(p.id, dark, p.hue)}>
					{@render tile(p.icon)}
					<span class="name">{p.name}</span>
					<button class="btn" disabled={busy[p.id]} onclick={() => toggleStack(p.id, false)}>
						<Play size={11} />Start
					</button>
				</div>
			{/each}
		</section>
	{/if}

	{#if rec}
		<section class="card rec {rec.icon}">
			<div class="eyebrow">Recommended</div>
			<div class="rec-body">
				<span class="rec-icon">
					{#if rec.icon === "update"}<Download size={16} />{:else}<CircleAlert size={16} />{/if}
				</span>
				<div class="rec-text">
					<b>{rec.title}</b>
					<span>{rec.detail}</span>
				</div>
				<button class="cta" onclick={() => send(rec.action)}>{rec.cta}</button>
			</div>
		</section>
	{/if}

	<footer>
		<button class="open" onclick={() => send({ kind: "open" })}>Open Atlas</button>
		<button class="gear" onclick={() => send({ kind: "settings" })} aria-label="Settings" title="Settings">
			<Settings size={15} />
		</button>
	</footer>
</div>

<style>
	/* The window is transparent over an NSVisualEffectView (Popover material): the page only
	   paints translucent cards, so the blur and the rounded corners come from the OS. */
	.pop {
		box-sizing: border-box;
		display: flex;
		flex-direction: column;
		gap: 10px;
		max-height: 100vh;
		overflow-y: auto;
		padding: 14px;
		border-radius: 14px;
		color: var(--ca-text-primary);
		--card: rgb(255 255 255 / 0.55);
		--card-line: rgb(0 0 0 / 0.07);
	}
	.pop.dark {
		--card: rgb(255 255 255 / 0.07);
		--card-line: rgb(255 255 255 / 0.1);
	}
	header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 2px 2px 0;
	}
	.mark {
		width: 30px;
		height: 30px;
		border-radius: 9px;
		background: linear-gradient(135deg, #c4b5fd, #8b5cf6 45%, #3b82f6);
		box-shadow: 0 1px 4px rgb(59 130 246 / 0.35);
	}
	.titles {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}
	.devchip {
		margin-left: 6px;
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 9.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: #b45309;
		background: rgb(245 158 11 / 0.18);
	}
	.titles b {
		font-size: 14px;
	}
	.state {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.state.fail {
		color: var(--dot-fail);
	}
	.dot {
		display: inline-block;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--dot-off);
		flex: none;
	}
	.dot.ok {
		background: var(--dot-ok);
	}
	.dot.warn {
		background: var(--dot-warn);
	}
	.dot.fail {
		background: var(--dot-fail);
	}
	.err {
		padding: 8px 10px;
		border-radius: 10px;
		font-size: 12px;
		background: color-mix(in oklch, var(--dot-fail) 16%, transparent);
		color: var(--dot-fail);
	}
	.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}
	.card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
		padding: 11px 12px;
		border: 1px solid var(--card-line);
		border-radius: 12px;
		background: var(--card);
		color: inherit;
		text-align: left;
		font: inherit;
	}
	button.card {
		cursor: default;
	}
	button.card:hover {
		background: color-mix(in srgb, var(--card), var(--ca-text-primary) 6%);
	}
	.project {
		border-top: 2px solid var(--c);
	}
	.hit {
		position: absolute;
		inset: 0;
		border: 0;
		background: none;
		border-radius: inherit;
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
	}
	.name {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		overflow: hidden;
		font-size: 12px;
		font-weight: 600;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dots {
		display: flex;
		flex-wrap: wrap;
		gap: 3px;
	}
	.meta {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.meta.attn {
		color: var(--dot-warn);
		font-weight: 600;
	}
	.big {
		font-size: 26px;
		font-weight: 650;
		letter-spacing: -0.02em;
	}
	.spark {
		pointer-events: none;
		height: 22px;
	}
	.actions {
		position: relative;
		display: flex;
		gap: 6px;
		margin-top: 2px;
	}
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 9px;
		border: 1px solid var(--card-line);
		border-radius: 999px;
		background: var(--card);
		color: inherit;
		font: inherit;
		font-size: 11px;
	}
	.btn.ghost {
		background: transparent;
	}
	.btn:disabled {
		opacity: 0.5;
	}
	.idle {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.idle-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 4px 4px 2px;
	}
	.idle-row .name {
		flex: 1;
		font-weight: 500;
		color: var(--ca-text-secondary);
	}
	/* Project identity tile: the same emoji / logo / monogram as the main window, in its accent. */
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
		color: white;
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
		gap: 6px;
	}
	.eyebrow {
		font-size: 10px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--ca-text-secondary);
	}
	.rec-body {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.rec-icon {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border-radius: 9px;
		background: color-mix(in oklch, var(--ca-brand-main) 18%, transparent);
		color: var(--ca-brand-main);
		flex: none;
	}
	.rec.fail .rec-icon {
		background: color-mix(in oklch, var(--dot-fail) 18%, transparent);
		color: var(--dot-fail);
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
		border-radius: 999px;
		background: var(--ca-brand-main);
		color: var(--ca-brand-contrast, #fff);
		font: inherit;
		font-size: 12px;
		font-weight: 600;
	}
	footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding-top: 2px;
	}
	.open {
		flex: 1;
		padding: 7px 12px;
		border: 1px solid var(--card-line);
		border-radius: 999px;
		background: var(--card);
		color: inherit;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
	}
	.gear {
		display: grid;
		place-items: center;
		width: 30px;
		height: 30px;
		border: 0;
		border-radius: 50%;
		background: transparent;
		color: var(--ca-text-secondary);
	}
	.gear:hover,
	.open:hover {
		background: var(--card);
	}
</style>
