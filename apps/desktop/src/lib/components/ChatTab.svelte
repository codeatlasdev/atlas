<script lang="ts">
	import { onMount, tick } from "svelte";
	import { listen } from "@tauri-apps/api/event";
	import { Spinner, toast } from "@codeatlasdev/ui";
	import { Square, Paperclip, ChevronRight, ChevronDown, Wrench, SquareTerminal, X } from "@lucide/svelte";
	import { marked } from "marked";
	import { host, b64bytes, b64encode } from "#lib/host.ts";
	import { errText } from "#lib/api.ts";
	import { apply, emptyChat, LineBuffer, type ChatState, type UiRequest } from "#lib/rpc.ts";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { worktreeFiles } from "#lib/stores/files.svelte.ts";

	let { tab, focused = false }: { tab: Tab; focused?: boolean } = $props();

	const STATE_POLL_MS = 10_000;
	const dec = new TextDecoder();

	let chat = $state<ChatState>(emptyChat());
	let lines = new LineBuffer();
	let scroller: HTMLDivElement | undefined = $state();
	let input: HTMLTextAreaElement | undefined = $state();
	let images = $state<{ name: string; data: string; mimeType: string }[]>([]);
	let open = $state<Record<string, boolean>>({});
	let history: string[] = [];
	let askedHistory = false;
	let hIdx = -1;
	let atFiles = $state<string[]>([]);
	let search = $state("");
	let searchOpen = $state(false);

	let text = $derived(tab.draft ?? "");
	let slash = $derived(
		text.startsWith("/") && !text.includes(" ")
			? chat.commands.filter((c) => c.name.startsWith(text.slice(1))).slice(0, 8)
			: [],
	);
	let atQuery = $derived(text.match(/(?:^|\s)@([^\s]*)$/)?.[1]);
	let atHits = $derived(atQuery === undefined ? [] : atFiles.filter((f) => f.includes(atQuery)).slice(0, 8));
	let shown = $derived(
		search
			? chat.items.filter((i) => "text" in i && i.text.toLowerCase().includes(search.toLowerCase()))
			: chat.items,
	);

	function send(cmd: Record<string, unknown>) {
		if (!tab.pty) return;
		void host.ptyWrite(tab.pty, `${JSON.stringify(cmd)}\n`).catch((e) => toast.error(errText(e)));
	}

	async function scrollDown() {
		await tick();
		if (scroller) scroller.scrollTop = scroller.scrollHeight;
	}

	function feed(bytes: Uint8Array, replay: boolean) {
		if (replay) {
			chat = emptyChat();
			lines.reset();
			askedHistory = false;
		}
		const nearBottom = !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80;
		for (const l of lines.push(dec.decode(bytes, { stream: !replay }))) apply(chat, l);
		chat = chat;
		if (chat.ready && !askedHistory) {
			askedHistory = true;
			send({ type: "get_messages", id: "history" });
		}
		if (chat.ready && tab.pendingPrompt) {
			const p = tab.pendingPrompt;
			tab.pendingPrompt = undefined;
			submit(p);
		}
		if (nearBottom) void scrollDown();
	}

	onMount(() => {
		let alive = true;
		const offs: (() => void)[] = [];
		listen<{ data: string; replay: boolean }>(`pty://${tab.pty}`, (e) =>
			feed(b64bytes(e.payload.data), e.payload.replay),
		).then((u) => (alive ? offs.push(u) : u()));
		const attach = () => tab.pty && host.ptyAttach(tab.pty, true).catch(() => {});
		void attach();
		listen("host://reconnected", attach).then((u) => (alive ? offs.push(u) : u()));
		const poll = () => send({ type: "get_state", id: "state" });
		const t = window.setInterval(poll, STATE_POLL_MS);
		window.setTimeout(poll, 1500);
		void worktreeFiles(tab.worktreeId).then((f) => (atFiles = f));
		return () => {
			alive = false;
			clearInterval(t);
			offs.forEach((u) => u());
			if (tab.pty) void host.ptyDetach(tab.pty).catch(() => {});
		};
	});

	$effect(() => {
		if (focused) input?.focus();
	});

	function submit(msg = text) {
		const m = msg.trim();
		if (!m && !images.length) return;
		send({
			type: "prompt",
			id: crypto.randomUUID(),
			message: m,
			...(images.length
				? { images: images.map((i) => ({ type: "image", data: i.data, mimeType: i.mimeType })) }
				: {}),
			...(chat.busy ? { streamingBehavior: "followUp" } : {}),
		});
		history = [m, ...history.filter((h) => h !== m)].slice(0, 50);
		hIdx = -1;
		images = [];
		if (msg === text) tab.draft = "";
		void scrollDown();
	}

	function answer(u: UiRequest, value: unknown) {
		send(
			value === undefined
				? { type: "extension_ui_response", id: u.id, cancelled: true }
				: { type: "extension_ui_response", id: u.id, value },
		);
		chat.ui = chat.ui.filter((x) => x.id !== u.id);
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
			e.preventDefault();
			if (slash.length && text !== `/${slash[0].name}`) tab.draft = `/${slash[0].name} `;
			else if (atHits.length) tab.draft = text.replace(/@[^\s]*$/, `@${atHits[0]} `);
			else submit();
		} else if (e.key === "ArrowUp" && !text.includes("\n") && history.length) {
			hIdx = Math.min(hIdx + 1, history.length - 1);
			tab.draft = history[hIdx];
			e.preventDefault();
		} else if (e.key === "ArrowDown" && hIdx >= 0) {
			hIdx -= 1;
			tab.draft = hIdx >= 0 ? history[hIdx] : "";
			e.preventDefault();
		} else if (e.key === "Escape" && chat.busy) send({ type: "abort" });
	}

	async function attach(files: FileList | null) {
		for (const f of files ?? []) {
			if (!f.type.startsWith("image/")) continue;
			images.push({ name: f.name, mimeType: f.type, data: b64encode(new Uint8Array(await f.arrayBuffer())) });
		}
	}

	function onwindowkey(e: KeyboardEvent) {
		if (focused && e.metaKey && e.key === "f") {
			searchOpen = !searchOpen;
			if (!searchOpen) search = "";
			e.preventDefault();
		}
	}

	const md = (s: string) => marked.parse(s, { async: false, breaks: true });
	const argsLine = (a: unknown) => {
		if (a && typeof a === "object") {
			const o = a as Record<string, unknown>;
			const q = Array.isArray(o.questions)
				? (o.questions[0] as Record<string, unknown> | undefined)?.question
				: undefined;
			return String(o.command ?? o.path ?? o.pattern ?? o.query ?? q ?? JSON.stringify(a)).slice(0, 160);
		}
		return "";
	};
</script>

<svelte:window onkeydown={onwindowkey} />

<div class="chat">
	<div class="bar">
		<span class="model">{chat.model || "omp"}</span>
		{#if chat.thinkingLevels.length}
			<select
				value={chat.thinking}
				onchange={(e) =>
					send({ type: "set_thinking_level", level: (e.currentTarget as HTMLSelectElement).value })}
			>
				{#each chat.thinkingLevels as l (l)}<option value={l}>{l}</option>{/each}
			</select>
		{/if}
		{#if chat.contextPercent !== null}<span class="dim">context {Math.round(chat.contextPercent)}%</span>{/if}
		{#if chat.status}<span class="dim">{chat.status}</span>{/if}
		<span class="spacer"></span>
		<button title="Compact the conversation" onclick={() => send({ type: "compact", id: "compact" })}
			>Compact</button
		>
		<button title="Raw protocol" onclick={() => (tab.rawView = true)}><SquareTerminal size={13} /></button>
	</div>
	{#if searchOpen}
		<!-- svelte-ignore a11y_autofocus -->
		<input class="search" autofocus placeholder="Search transcript" bind:value={search} />
	{/if}
	<div class="transcript" bind:this={scroller}>
		{#if !chat.ready}<div class="center"><Spinner /> Starting omp…</div>{/if}
		{#each shown as it (it.id)}
			{#if it.kind === "user"}
				<div class="msg user selectable">
					{it.text}{#if it.images}<span class="dim"> · {it.images} image(s)</span>{/if}
				</div>
			{:else if it.kind === "assistant"}
				<div class="msg assistant">
					{#if it.thinking}
						<button class="fold" onclick={() => (open[`th-${it.id}`] = !open[`th-${it.id}`])}>
							{#if open[`th-${it.id}`]}<ChevronDown size={12} />{:else}<ChevronRight size={12} />{/if} Thinking
						</button>
						{#if open[`th-${it.id}`]}<pre class="thinking selectable">{it.thinking}</pre>{/if}
					{/if}
					{#if it.text}<div class="md selectable">{@html md(it.text)}</div>{/if}
					{#each it.tools as id (id)}
						{@const tc = chat.tools[id]}
						{#if tc}
							<div class="tool {tc.status}">
								<button class="fold" onclick={() => (open[id] = !open[id])}>
									{#if tc.status === "running"}<Spinner size={11} />{:else}<Wrench size={11} />{/if}
									<b>{tc.name}</b> <span class="mono">{argsLine(tc.args)}</span>
								</button>
								{#if open[id] && tc.result}<pre class="selectable">{tc.result}</pre>{/if}
							</div>
						{/if}
					{/each}
					{#if it.streaming && !it.text && !it.tools.length}<Spinner size={12} />{/if}
				</div>
			{:else if it.kind === "note"}
				<div class="msg note">{it.text}</div>
			{:else}
				<div class="msg error selectable">{it.text}</div>
			{/if}
		{/each}
		{#each chat.ui as u (u.id)}
			<div class="ask">
				<b>{u.title}</b>
				{#if u.message}<p>{u.message}</p>{/if}
				{#if u.method === "select"}
					<div class="opts">
						{#each u.options ?? [] as o (o)}<button onclick={() => answer(u, o)}>{o}</button>{/each}
					</div>
				{:else if u.method === "confirm"}
					<div class="opts">
						<button onclick={() => answer(u, true)}>Yes</button>
						<button onclick={() => answer(u, false)}>No</button>
					</div>
				{:else}
					<textarea
						placeholder={u.placeholder ?? ""}
						onkeydown={(e) => {
							if (e.key === "Enter" && (e.metaKey || u.method === "input")) {
								e.preventDefault();
								answer(u, (e.currentTarget as HTMLTextAreaElement).value);
							}
						}}></textarea>
				{/if}
				<button class="cancel" onclick={() => answer(u, undefined)}>Dismiss</button>
			</div>
		{/each}
	</div>
	<div class="composer">
		{#if slash.length || atHits.length}
			<div class="suggest">
				{#each slash as c (c.name)}
					<button onclick={() => (tab.draft = `/${c.name} `)}
						><b>/{c.name}</b> <span class="dim">{c.description}</span></button
					>
				{/each}
				{#each atHits as f (f)}
					<button onclick={() => (tab.draft = text.replace(/@[^\s]*$/, `@${f} `))}>@{f}</button>
				{/each}
			</div>
		{/if}
		{#if images.length}
			<div class="imgs">
				{#each images as im, i (i)}
					<span class="chip"
						>{im.name}<button onclick={() => images.splice(i, 1)}><X size={10} /></button></span
					>
				{/each}
			</div>
		{/if}
		<div class="row">
			<label class="attach" title="Attach image">
				<Paperclip size={14} />
				<input
					type="file"
					accept="image/*"
					multiple
					hidden
					onchange={(e) => attach((e.currentTarget as HTMLInputElement).files)}
				/>
			</label>
			<textarea
				bind:this={input}
				rows="2"
				value={text}
				oninput={(e) => (tab.draft = (e.currentTarget as HTMLTextAreaElement).value)}
				{onkeydown}
				placeholder={chat.busy ? "Queue a follow-up (Esc to stop)" : "Message omp — / commands, @ files"}
			></textarea>
			{#if chat.busy}
				<button class="stop" title="Stop (Esc)" onclick={() => send({ type: "abort" })}
					><Square size={12} fill="currentColor" /></button
				>
			{/if}
		</div>
	</div>
</div>

<style>
	.chat {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		font-size: 13px;
	}
	.bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		font-size: 12px;
	}
	.bar select {
		font: inherit;
		font-size: 11.5px;
	}
	.bar button {
		border: 0;
		background: none;
		font: inherit;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		display: inline-flex;
	}
	.model {
		font-weight: 600;
	}
	.spacer {
		flex: 1;
	}
	.search {
		margin: 6px 10px;
		font: inherit;
		font-size: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: inherit;
	}
	.transcript {
		flex: 1;
		overflow-y: auto;
		padding: 12px 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.center {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--ca-text-secondary);
	}
	.msg {
		max-width: 860px;
		line-height: 1.55;
	}
	.user {
		align-self: flex-end;
		white-space: pre-wrap;
		padding: 8px 12px;
		border-radius: 10px;
		background: color-mix(in oklab, var(--ca-brand-main) 14%, transparent);
	}
	.note {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
		border-left: 2px solid var(--ca-outline-subtle);
		padding-left: 8px;
	}
	.error {
		color: var(--ca-semantic-error, #d33);
		white-space: pre-wrap;
	}
	.md :global(pre) {
		overflow-x: auto;
		padding: 8px;
		border-radius: 6px;
		background: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
	}
	.md :global(p) {
		margin: 0.4em 0;
	}
	.fold {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		padding: 2px 0;
		text-align: left;
		max-width: 100%;
	}
	.thinking,
	.tool pre {
		margin: 4px 0 4px 16px;
		max-height: 260px;
		overflow: auto;
		font-size: 11.5px;
		white-space: pre-wrap;
		color: var(--ca-text-secondary);
	}
	.tool {
		border-left: 2px solid var(--ca-outline-subtle);
		padding-left: 8px;
		margin: 4px 0;
	}
	.tool.error {
		border-left-color: var(--ca-semantic-error, #d33);
	}
	.mono {
		font-family: var(--mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ask {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border-radius: 10px;
		border: 1px solid var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 8%, transparent);
		max-width: 640px;
	}
	.ask p {
		margin: 0;
		white-space: pre-wrap;
	}
	.opts {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.opts button,
	.cancel {
		font: inherit;
		font-size: 12px;
		padding: 4px 10px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base);
		color: inherit;
	}
	.cancel {
		align-self: flex-start;
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
	}
	.composer {
		border-top: 0.5px solid var(--ca-outline-subtle);
		padding: 8px 10px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.suggest {
		display: flex;
		flex-direction: column;
		max-height: 200px;
		overflow-y: auto;
	}
	.suggest button {
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: inherit;
		text-align: left;
		padding: 3px 6px;
		border-radius: 5px;
	}
	.suggest button:hover {
		background: var(--ca-action-hover);
	}
	.imgs {
		display: flex;
		gap: 6px;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		padding: 2px 6px;
		border-radius: 5px;
		background: var(--ca-action-hover);
	}
	.chip button {
		border: 0;
		background: none;
		display: inline-flex;
		color: inherit;
	}
	.row {
		display: flex;
		align-items: flex-end;
		gap: 6px;
	}
	textarea {
		flex: 1;
		resize: none;
		font: inherit;
		font-size: 13px;
		padding: 8px 10px;
		border-radius: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 3%, transparent);
		color: inherit;
		max-height: 200px;
	}
	.attach {
		display: inline-flex;
		padding: 8px 4px;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.stop {
		border: 0;
		border-radius: 8px;
		padding: 9px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		display: inline-flex;
	}
	.dim {
		color: var(--ca-text-secondary);
		font-size: 11.5px;
	}
</style>
