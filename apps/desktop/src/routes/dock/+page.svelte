<script lang="ts">
	import { colorScheme } from "@codeatlasdev/ui";
	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { getCurrentWindow } from "@tauri-apps/api/window";
	import { ArrowUpRight, Bot, CornerDownLeft, EyeOff, Paperclip, TriangleAlert, X } from "@lucide/svelte";
	import { onMount, tick, untrack } from "svelte";
	import { errText } from "#lib/api.ts";
	import {
		EMPTY_DOCK_SNAPSHOT,
		agentLabel,
		initialState,
		reduce,
		replyOf,
		resolveTarget,
		windowSize,
		type DockAttachment,
		type DockEvent,
		type DockEffect,
		type DockPrefs,
		type DockSnapshot,
		type DockState,
	} from "#lib/dock.ts";
	import { kbd } from "#lib/keys.ts";

	let snap = $state<DockSnapshot>(EMPTY_DOCK_SNAPSHOT);
	let prefs = $state<DockPrefs>({ enabled: true, summon: "ctrl-alt-space", toggle: "ctrl-alt-d" });
	let st = $state<DockState>(initialState());
	let root = $state<HTMLDivElement>();
	let input = $state<HTMLTextAreaElement>();

	let agents = $derived(snap.agents);
	let target = $derived(resolveTarget({ text: st.text, chip: st.chip, sticky: st.sticky, agents }));
	let current = $derived(
		agents.find(
			(a) => a.tabId === (target.kind === "target" || target.kind === "empty" ? target.tabId : undefined),
		),
	);
	let replying = $derived(agents.find((a) => a.tabId === st.replyTab));
	let asking = $derived(agents.find((a) => a.tabId === st.needs));
	let busyAgents = $derived(agents.filter((a) => a.state === "working").length);
	let reply = $derived(replying ? replyOf(replying) : { lines: [], busy: false });

	function dispatch(ev: DockEvent) {
		const r = reduce(st, ev);
		st = r.state;
		for (const e of r.effects) void run(e);
	}

	async function run(e: DockEffect) {
		switch (e.type) {
			case "focusComposer":
				await invoke("dock_keys", { on: true });
				await tick();
				input?.focus();
				break;
			case "deliver":
				try {
					await invoke("dock_send", { tab: e.tabId, text: e.text });
					dispatch({ type: "sendOk", now: Date.now() });
				} catch (err) {
					dispatch({ type: "sendErr", message: errText(err) });
				}
				break;
			case "openInAtlas":
				await invoke("dock_open", { tab: e.tabId });
				break;
		}
	}

	/** Theme follows the choice made in the main window (shared localStorage), like the tray popover. */
	function syncTheme() {
		const pref = JSON.parse(localStorage.getItem("ca-scheme") ?? '"system"');
		colorScheme.setPreference(pref);
		void getCurrentWindow()
			.setTheme(pref === "system" ? null : pref)
			.catch(() => {});
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === "Escape") return dispatch({ type: "esc" });
		if (st.mode === "composer" && (e.metaKey || e.ctrlKey) && e.key === "Enter") {
			e.preventDefault();
			return dispatch({ type: "send", agents });
		}
		if (st.mode === "composer" && e.key === "Enter" && !e.shiftKey && !e.isComposing) {
			e.preventDefault();
			return dispatch({ type: "send", agents });
		}
		if (st.mode === "ask" && /^[1-9]$/.test(e.key)) {
			const c = st.candidates[Number(e.key) - 1];
			if (c) dispatch({ type: "choose", tabId: c.tabId });
		}
	}

	/** Captures sent from the quick card wait in Rust until the page asks (it may not have been listening yet). */
	async function takeAttachments() {
		try {
			for (const a of await invoke<DockAttachment[]>("dock_take_attachments")) {
				dispatch({ type: "attach", path: a.path, kind: a.kind });
			}
		} catch (err) {
			dispatch({ type: "sendErr", message: errText(err) });
		}
	}

	onMount(() => {
		const offs: Array<() => void> = [];
		const keep = (p: Promise<() => void>) => void p.then((f) => offs.push(f));
		keep(
			listen<DockSnapshot>("dock://snapshot", (e) => {
				snap = e.payload;
			}),
		);
		keep(listen<DockPrefs>("dock://prefs", (e) => (prefs = e.payload)));
		keep(listen("dock://summon", () => dispatch({ type: "summon" })));
		keep(listen("dock://attach", () => void takeAttachments()));
		void invoke<DockSnapshot | null>("dock_snapshot").then((s) => {
			if (s) snap = s;
		});
		void invoke<DockPrefs>("dock_prefs").then((p) => (prefs = p));
		void takeAttachments();
		syncTheme();
		window.addEventListener("keydown", onKey);
		// The page is painted: the pill may appear (Rust shows it only for the user's app, and only when enabled).
		void tick().then(() => invoke("dock_ready"));
		const clock = window.setInterval(
			() => dispatch({ type: "tick", now: Date.now(), agents: snap.agents }),
			1000,
		);
		return () => {
			offs.forEach((f) => f());
			window.removeEventListener("keydown", onKey);
			window.clearInterval(clock);
		};
	});

	// Agents moved: the card, the badge and the reply follow them.
	$effect(() => {
		const now = agents;
		untrack(() => dispatch({ type: "agents", agents: now }));
	});

	// The composer owns the keyboard only while it is open; closed, the pill never takes it from the app you are in.
	$effect(() => {
		const typing = st.mode === "composer" || st.mode === "ask" || st.mode === "error";
		if (!typing) void invoke("dock_keys", { on: false });
	});

	// One window, sized per mode and grown to its content.
	$effect(() => {
		void [
			st.mode,
			st.text,
			st.error,
			st.candidates.length,
			reply.lines.length,
			st.needs,
			st.attachments.length,
		];
		void fit();
	});

	async function fit() {
		await tick();
		const base = windowSize(st.mode);
		const content = root?.scrollHeight ?? 0;
		void invoke("dock_resize", { width: base.width, height: Math.max(base.height, Math.ceil(content)) });
	}

	const hint = $derived(kbd(prefs.summon || "ctrl-alt-space"));

	/** Hide the pill and remember it; the hide hotkey, the tray menu and Settings bring it back. */
	function hideDock() {
		void invoke("dock_toggle").catch(() => {});
	}

	/** The native right-click menu (a webview menu would need the panel to become key). */
	function onPillMenu(e: MouseEvent) {
		e.preventDefault();
		void invoke("dock_menu").catch(() => {});
	}
</script>

<svelte:head><title>Atlas Dock</title></svelte:head>

<div
	class="dock"
	bind:this={root}
	data-mode={st.mode}
	data-tauri-drag-region={st.mode === "pill" ? "" : undefined}
>
	{#if st.mode === "pill" || st.mode === "sending"}
		<div class="pillrow" oncontextmenu={onPillMenu} role="presentation">
			<button class="pill" onclick={() => dispatch({ type: "summon" })} aria-label="Message an agent">
				<span class="dot" data-state={busyAgents > 0 ? "working" : "idle"} aria-hidden="true"></span>
				<span class="who">
					{#if st.mode === "sending"}Sending…{:else if current}{agentLabel(current)}{:else}No agent running{/if}
				</span>
				{#if st.mode === "pill"}<kbd>{hint}</kbd>{/if}
				{#if st.attention > 0}<span class="badge" aria-label="{st.attention} need you">{st.attention}</span
					>{/if}
			</button>
			<button
				class="hide"
				onclick={hideDock}
				aria-label="Hide the dock"
				title={prefs.toggle
					? `Hide the dock (${kbd(prefs.toggle)} brings it back)`
					: "Hide the dock (the tray menu brings it back)"}
			>
				<EyeOff size={13} aria-hidden="true" />
			</button>
		</div>
	{:else if st.mode === "composer" || st.mode === "ask" || st.mode === "error"}
		<div class="card">
			<div class="to">
				<Bot size={14} aria-hidden="true" />
				<label for="dock-to">To</label>
				<select
					id="dock-to"
					value={st.chip ?? ""}
					onchange={(e) => dispatch({ type: "pin", tabId: e.currentTarget.value || null })}
					disabled={agents.length === 0}
				>
					<option value="">
						{current ? `${agentLabel(current)} (auto)` : "No agent running"}
					</option>
					{#each agents as a (a.tabId)}
						<option value={a.tabId}>{agentLabel(a)}</option>
					{/each}
				</select>
				<button class="x" onclick={() => dispatch({ type: "esc" })} aria-label="Close"><X size={14} /></button
				>
			</div>
			<textarea
				bind:this={input}
				rows="3"
				placeholder="Tell the agent what to do. Mention one with @name."
				value={st.text}
				oninput={(e) => dispatch({ type: "edit", text: e.currentTarget.value })}></textarea>
			{#if st.attachments.length > 0}
				<ul class="chips" aria-label="Attached captures">
					{#each st.attachments as a (a.path)}
						<li>
							<Paperclip size={12} aria-hidden="true" />
							<span title={a.path}>{a.path.split("/").pop()}</span>
							<button
								onclick={() => dispatch({ type: "detach", path: a.path })}
								aria-label="Remove {a.path.split('/').pop()}"><X size={12} /></button
							>
						</li>
					{/each}
				</ul>
			{/if}
			{#if st.mode === "ask"}
				<div class="ask" role="group" aria-label="Which agent?">
					<span>Several agents match. Which one?</span>
					{#each st.candidates as c, i (c.tabId)}
						<button onclick={() => dispatch({ type: "choose", tabId: c.tabId })}>
							<kbd>{i + 1}</kbd>
							{agentLabel(c)}
						</button>
					{/each}
				</div>
			{/if}
			{#if st.mode === "error" && st.error}
				<p class="err" role="alert"><TriangleAlert size={13} aria-hidden="true" /> {st.error}</p>
			{/if}
			<div class="foot">
				<span>Esc closes</span>
				<button
					class="send"
					disabled={st.text.trim() === "" && st.attachments.length === 0}
					onclick={() => dispatch({ type: "send", agents })}
				>
					Send <CornerDownLeft size={13} aria-hidden="true" />
				</button>
				{#if agents.length === 0}
					<button class="link" onclick={() => dispatch({ type: "openInAtlas" })}>Open Atlas</button>
				{/if}
			</div>
		</div>
	{:else if st.mode === "replying"}
		<div class="card">
			<div class="to">
				<span class="dot" data-state={reply.busy ? "working" : "idle"} aria-hidden="true"></span>
				<strong>{replying ? agentLabel(replying) : "Agent"}</strong>
				<span class="state">{reply.busy ? "working" : (replying?.state ?? "")}</span>
				<button class="link" onclick={() => dispatch({ type: "openInAtlas" })}>
					Open in Atlas <ArrowUpRight size={12} aria-hidden="true" />
				</button>
			</div>
			<div class="lines" aria-live="polite">
				{#each reply.lines as l, i (i)}<p>{l}</p>{:else}<p class="dim">Waiting for the agent…</p>{/each}
			</div>
		</div>
	{:else if st.mode === "needsInput"}
		<div class="card need">
			<div class="to">
				<TriangleAlert size={14} aria-hidden="true" />
				<strong>{asking ? agentLabel(asking) : "An agent"} needs you</strong>
				<button class="x" onclick={() => dispatch({ type: "esc" })} aria-label="Dismiss"
					><X size={14} /></button
				>
			</div>
			<div class="lines">
				{#each replyOf(asking ?? { lastMessage: null, state: "waiting" }).lines as l, i (i)}<p>{l}</p>{/each}
			</div>
			<div class="foot">
				<span></span>
				<button class="send" onclick={() => dispatch({ type: "openInAtlas" })}>
					Open in Atlas <ArrowUpRight size={13} aria-hidden="true" />
				</button>
			</div>
		</div>
	{/if}
</div>

<style>
	:global(html),
	:global(body) {
		background: transparent !important;
	}
	.dock {
		box-sizing: border-box;
		width: 100vw;
		color: var(--ca-text-primary);
		font-size: 12.5px;
		--line: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
	}
	.pill,
	.card {
		box-sizing: border-box;
		width: 100%;
		border: 0;
		color: inherit;
		background: var(--ca-surface-raised);
		box-shadow:
			inset 0 0 0 1px var(--line),
			0 6px 24px color-mix(in oklab, black 28%, transparent);
	}
	.pillrow {
		position: relative;
	}
	/* Inside the pill's right edge, invisible until the pointer is over the pill (or it has focus): the idle pill
	   stays quiet and the window needs no spare room around it. */
	.hide {
		position: absolute;
		top: 50%;
		right: 8px;
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		padding: 0;
		border: 0;
		border-radius: 50%;
		color: var(--ca-text-secondary);
		background: var(--ca-surface-raised);
		box-shadow: inset 0 0 0 1px var(--line);
		cursor: pointer;
		opacity: 0;
		transform: translateY(-50%);
	}
	.pillrow:hover .hide,
	.hide:focus-visible {
		opacity: 1;
	}
	.hide:hover {
		color: var(--ca-text-primary);
	}
	.pill {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 36px;
		padding: 0 14px;
		border-radius: 18px;
		cursor: pointer;
		font: inherit;
	}
	.who {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-align: left;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	kbd {
		padding: 1px 6px;
		border-radius: 5px;
		font: inherit;
		font-size: 11px;
		color: var(--ca-text-secondary);
		box-shadow: inset 0 0 0 1px var(--line);
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-text-tertiary, var(--line));
	}
	.dot[data-state="working"] {
		background: var(--ca-semantic-success);
	}
	.badge {
		min-width: 18px;
		padding: 0 5px;
		border-radius: 9px;
		text-align: center;
		font-size: 11px;
		color: var(--ca-surface-sunken);
		background: var(--ca-semantic-warning);
	}
	.card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border-radius: 14px;
	}
	.card.need {
		box-shadow:
			inset 0 0 0 1px var(--ca-semantic-warning),
			0 6px 24px color-mix(in oklab, black 28%, transparent);
	}
	.to,
	.foot,
	.ask {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.to select {
		flex: 1;
		min-width: 0;
		color: inherit;
		font: inherit;
		background: transparent;
		border: 0;
	}
	.to .x,
	.link,
	.ask button {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		color: var(--ca-text-secondary);
		font: inherit;
		background: transparent;
		border: 0;
		cursor: pointer;
	}
	.to strong {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.state {
		color: var(--ca-text-secondary);
	}
	textarea {
		box-sizing: border-box;
		width: 100%;
		resize: none;
		padding: 8px 10px;
		color: inherit;
		font: inherit;
		background: var(--ca-surface-sunken);
		border: 0;
		border-radius: 8px;
		box-shadow: inset 0 0 0 1px var(--line);
		outline: none;
	}
	textarea:focus-visible {
		box-shadow: inset 0 0 0 1px var(--ca-brand-main);
	}
	.ask {
		flex-wrap: wrap;
	}
	.err {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		color: var(--ca-semantic-error);
	}
	.foot {
		justify-content: space-between;
		color: var(--ca-text-secondary);
	}
	.send {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 10px;
		color: var(--ca-surface-sunken);
		font: inherit;
		background: var(--ca-brand-main);
		border: 0;
		border-radius: 7px;
		cursor: pointer;
	}
	.send:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.lines {
		display: flex;
		flex-direction: column;
		gap: 2px;
		max-height: 120px;
		overflow: hidden;
	}
	.lines p {
		margin: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.dim {
		color: var(--ca-text-secondary);
	}
</style>
