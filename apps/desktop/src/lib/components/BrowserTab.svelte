<script lang="ts">
	import { onMount } from "svelte";
	import { kbd, primaryKey } from "#lib/keys.ts";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { DropdownMenu, DropdownMenuItem, toast } from "@codeatlasdev/ui";
	import { ArrowLeft, ArrowRight, RotateCw, ExternalLink, MousePointerClick, WifiOff } from "@lucide/svelte";
	import { api, errText } from "#lib/api.ts";
	import { toUrl, rememberUrl } from "#lib/stores/browser.svelte.ts";
	import { browserSurface } from "#lib/browserSync.ts";
	import { idle, step, type LoadEvent } from "#lib/browserLoad.ts";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { grabPrompt, type Grab } from "#lib/grab.ts";
	import { nativeRect } from "#lib/zoom.ts";
	import { watchSlot } from "#lib/native/slot.ts";

	let { tab, visible }: { tab: Tab; visible: boolean } = $props();

	let area: HTMLDivElement | undefined = $state();
	let address = $state("");
	let editing = $state(false);
	let picking = $state(false);
	let grabbed = $state<{ grab: Grab; shot: string | null } | null>(null);
	/** Load state from the native events (`browserLoad.ts`): a failed load draws the tab's own screen. */
	let load = $state(idle);
	const failure = $derived(load.failure);
	/** The page's load events drive the failure model here and the core's loading strip there. */
	function feed(e: LoadEvent) {
		load = step(load, e);
		if (e.event === "nav" || e.event === "reload") pane.loading.start();
		else if (e.event === "load") pane.loading.finish();
		else if (e.event === "fail") pane.loading.fail();
		sync();
	}

	$effect(() => {
		if (!editing) address = tab.url ?? "";
	});

	/** The pane in window points (what the native child webview's frame takes; the slot itself is in CSS px). */
	function rect() {
		const r = area!.getBoundingClientRect();
		return nativeRect({ x: r.left, y: r.top, w: r.width, h: r.height });
	}

	// The native-view core (`lib/native/surface.ts`) serializes open → bounds → park per tab, measures when each call
	// runs (a stale rect can never win), carves the page transparent over the page, parks for swipes, reserves the 2 px
	// loading strip above the page and owns the stall timer; this file only says how a browser page is opened.
	// `{#each (t.id)}` keys this component by tab id: the id never changes for its lifetime.
	// svelte-ignore state_referenced_locally
	const pane = browserSurface({
		tab: tab.id,
		invoke,
		slot: () => area,
		shown: () => visible,
		// A failed page is parked behind the tab's own error screen, never a white rectangle.
		failed: () => !!failure,
		url: () => tab.url,
		onError: (e) => toast.error(errText(e)),
		// A load that neither finishes nor fails (and, off macOS, every failure: no native `fail` event there) ends as a timeout.
		onStall: () => feed({ event: "stall", url: load.url }),
	});
	const sync = () => void pane.sync();

	$effect(() => {
		void visible;
		sync();
	});

	onMount(() => {
		const unwatch = watchSlot(area!, sync);
		const off = listen<LoadEvent | { event: "title"; title?: string; url?: string }>(
			`browser://${tab.id}`,
			(e) => {
				const p = e.payload;
				if (p.event === "title") {
					// A title arriving is the page being alive: a slow valid page is never marked stalled.
					pane.loading.progress();
					if (p.title && !tab.customLabel) tab.label = p.title.slice(0, 60);
					return;
				}
				feed(p);
				if ((p.event === "load" || p.event === "nav") && p.url) {
					tab.url = p.url;
					rememberUrl(tab.worktreeId, p.url);
				}
			},
		);
		return () => {
			unwatch();
			void off.then((u) => u());
			// Parked, not closed: the tab may only be re-parented (split/move). closeTab closes it.
			void pane.dispose();
		};
	});

	async function go() {
		editing = false;
		const url = toUrl(address);
		tab.url = url;
		rememberUrl(tab.worktreeId, url);
		if (pane.isOpen())
			await invoke("browser_navigate", { tab: tab.id, url }).catch((e) => toast.error(errText(e)));
		else sync();
	}

	const nav = (action: "back" | "forward" | "reload") => {
		if (action === "reload") feed({ event: "reload" });
		return invoke("browser_nav", { tab: tab.id, action }).catch((e) => toast.error(errText(e)));
	};

	/** Screenshot of the picked element (window capture cropped to its rect). */
	async function screenshot(g: Grab): Promise<string | null> {
		const r = rect();
		const t = g.target.rectViewport;
		return invoke<string>("window_region_png", {
			x: r.x + t.x,
			y: r.y + t.y,
			w: Math.max(1, t.width),
			h: Math.max(1, t.height),
		}).catch(() => null);
	}

	async function pick() {
		if (picking) {
			picking = false;
			await invoke("browser_pick_cancel", { tab: tab.id }).catch(() => {});
			return;
		}
		picking = true;
		try {
			const grab = await invoke<Grab>("browser_pick", { tab: tab.id });
			grabbed = { grab, shot: await screenshot(grab) };
		} catch (e) {
			if (errText(e) !== "cancelled") toast.error(errText(e));
		} finally {
			picking = false;
		}
	}

	async function send(tabId: string | null) {
		if (!grabbed) return;
		const text = grabPrompt(grabbed.grab, grabbed.shot);
		if (tabId) await agents.send(tabId, text);
		else await agents.launch(tab.worktreeId, agents.defaultAgent, { prompt: text });
		grabbed = null;
		toast.success("Sent to agent");
	}

	function onkeydown(e: KeyboardEvent) {
		if (!visible) return;
		if (primaryKey(e) && e.key === "l") {
			e.preventDefault();
			document.getElementById(`addr-${tab.id}`)?.focus();
		}
		if (e.key === "Escape" && picking) void pick();
	}
</script>

<svelte:window {onkeydown} />

<div class="browser">
	<div class="bar">
		<button title="Back" onclick={() => nav("back")}><ArrowLeft size={14} /></button>
		<button title="Forward" onclick={() => nav("forward")}><ArrowRight size={14} /></button>
		<button title="Reload" onclick={() => nav("reload")}><RotateCw size={13} /></button>
		<input
			id="addr-{tab.id}"
			list="hist-{tab.id}"
			bind:value={address}
			onfocus={(e) => {
				editing = true;
				(e.currentTarget as HTMLInputElement).select();
			}}
			onblur={() => (editing = false)}
			onkeydown={(e) => e.key === "Enter" && go()}
			placeholder={`Search or enter address (${kbd("cmd-l")})`}
		/>
		<datalist id="hist-{tab.id}">
			{#each ws.doc.urlHistory[tab.worktreeId] ?? [] as u (u)}<option value={u}></option>{/each}
		</datalist>
		<button class:on={picking} title="Design Mode: pick an element for the agent" onclick={pick}>
			<MousePointerClick size={14} />
		</button>
		<button title="Open in system browser" onclick={() => tab.url && api.openUrl(tab.url)}>
			<ExternalLink size={13} />
		</button>
	</div>
	{#if grabbed}
		<div class="grabbed">
			<code>{grabbed.grab.target.selector}</code>
			{#if grabbed.shot}<span class="dim">+ screenshot</span>{/if}
			<span class="spacer"></span>
			<DropdownMenu align="end">
				{#snippet trigger()}<span class="send">Send to agent</span>{/snippet}
				{#each agents.tabsIn(tab.worktreeId) as t (t.id)}
					<DropdownMenuItem onclick={() => send(t.id)}
						>{(t.pty && ws.titles[t.pty]) || t.label}</DropdownMenuItem
					>
				{/each}
				<DropdownMenuItem onclick={() => send(null)}>New {agents.defaultAgent} session</DropdownMenuItem>
			</DropdownMenu>
			<button onclick={() => (grabbed = null)}>Discard</button>
		</div>
	{/if}
	{#if picking}<div class="hint">Click an element in the page · Esc to cancel</div>{/if}
	<div class="area" bind:this={area}>
		<!-- The slot's own colour; the core carves the page's hole out of it (never out of the slot). -->
		<div class="fill" data-native-fill></div>
		{#if !tab.url}
			<div class="state">
				<p class="title">New tab</p>
				<p class="dim">Type an address or a search above ({kbd("cmd-l")}).</p>
			</div>
		{:else if failure}
			<div class="state">
				<WifiOff size={28} strokeWidth={1.5} />
				<p class="title">{failure.title}</p>
				<p class="dim url">{failure.url || tab.url}</p>
				<p class="dim">{failure.hint}</p>
				<div class="actions">
					<button class="primary" onclick={() => nav("reload")}><RotateCw size={13} /> Try again</button>
					<button onclick={() => tab.url && api.openUrl(tab.url)}
						><ExternalLink size={13} /> Open in browser</button
					>
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.browser {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.bar {
		position: relative;
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 8px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.bar button,
	.grabbed button {
		display: inline-flex;
		padding: 4px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
	}
	.bar button.on {
		color: var(--ca-brand-contrast);
		background: var(--ca-brand-fill);
	}
	input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12px;
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 4%, transparent);
		color: inherit;
	}
	.grabbed,
	.hint {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-brand-main) 10%, transparent);
	}
	.spacer {
		flex: 1;
	}
	.send {
		padding: 3px 10px;
		border-radius: 6px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		font-weight: 600;
	}
	.area {
		position: relative;
		flex: 1;
		min-height: 0;
	}
	.fill {
		position: absolute;
		inset: 0;
		pointer-events: none;
		background: var(--ca-surface-base);
	}
	.state {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 24px;
		text-align: center;
		color: var(--ca-text-secondary);
	}
	.state .title {
		margin: 4px 0 0;
		font-size: 15px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.state p {
		margin: 0;
		font-size: 12.5px;
		max-width: 420px;
	}
	.state .url {
		font-family: var(--ca-font-mono, ui-monospace, monospace);
		font-size: 12px;
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		gap: 8px;
		margin-top: 10px;
	}
	.actions button {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-primary);
		font: inherit;
		font-size: 12.5px;
	}
	.actions button.primary {
		border-color: transparent;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.dim {
		color: var(--ca-text-secondary);
	}
</style>
