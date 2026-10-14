<script lang="ts">
	/**
	 * Native chat for an omp pipe-mode agent: reads the pty's JSONL stream into `ChatState`
	 * (see `rpc.ts`) and writes commands back. The transcript and composer are in `chat/`.
	 */
	import { onMount } from "svelte";
	import { primaryKey } from "#lib/keys.ts";
	import { listen } from "@tauri-apps/api/event";
	import { toast } from "@codeatlasdev/ui";
	import { SquareTerminal } from "@lucide/svelte";
	import { host, b64bytes } from "#lib/host.ts";
	import { errText } from "#lib/api.ts";
	import { apply, emptyChat, LineBuffer, type ChatState, type UiRequest } from "#lib/rpc.ts";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { worktreeFiles } from "#lib/stores/files.svelte.ts";
	import Composer, { type Pasted } from "./chat/Composer.svelte";
	import { pushHistory } from "./chat/composer.ts";
	import Transcript from "./chat/Transcript.svelte";

	let { tab, focused = false }: { tab: Tab; focused?: boolean } = $props();

	const STATE_POLL_MS = 10_000;
	const dec = new TextDecoder();

	let chat = $state<ChatState>(emptyChat());
	let lines = new LineBuffer();
	let transcript: Transcript | undefined = $state();
	let images = $state<Pasted[]>([]);
	let history: string[] = [];
	let askedHistory = false;
	let hIdx = -1;
	let atFiles = $state<string[]>([]);
	let search = $state("");
	let searchOpen = $state(false);

	let shown = $derived(
		search
			? chat.items.filter((i) => "text" in i && i.text.toLowerCase().includes(search.toLowerCase()))
			: chat.items,
	);

	function send(cmd: Record<string, unknown>) {
		if (!tab.pty) return;
		void host.ptyWrite(tab.pty, `${JSON.stringify(cmd)}\n`).catch((e) => toast.error(errText(e)));
	}

	function feed(bytes: Uint8Array, replay: boolean) {
		if (replay) {
			chat = emptyChat();
			lines.reset();
			askedHistory = false;
		}
		const follow = transcript?.nearBottom() ?? true;
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
		if (follow) void transcript?.scrollDown();
	}

	onMount(() => {
		let alive = true;
		const offs: (() => void)[] = [];
		const ready = listen<{ data: string; replay: boolean }>(`pty://${tab.pty}`, (e) =>
			feed(b64bytes(e.payload.data), e.payload.replay),
		).then((u) => (alive ? offs.push(u) : u()));
		// After the listener is live: the replay hostd sends at once would otherwise be lost (see ptyStream.ts).
		const attach = () =>
			void ready
				.then(() => {
					if (alive && tab.pty) return host.ptyAttach(tab.pty, true);
				})
				.catch(() => {});
		attach();
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

	function submit(msg = tab.draft ?? "") {
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
		history = pushHistory(history, m);
		hIdx = -1;
		images = [];
		if (msg === (tab.draft ?? "")) tab.draft = "";
		void transcript?.scrollDown();
	}

	/** Step through sent prompts with ↑/↓; `true` when the key was used. */
	function stepHistory(dir: "up" | "down"): boolean {
		if (dir === "up") {
			if (!history.length) return false;
			hIdx = Math.min(hIdx + 1, history.length - 1);
		} else {
			if (hIdx < 0) return false;
			hIdx -= 1;
		}
		tab.draft = hIdx >= 0 ? history[hIdx] : "";
		return true;
	}

	function answer(u: UiRequest, value: unknown) {
		send(
			value === undefined
				? { type: "extension_ui_response", id: u.id, cancelled: true }
				: { type: "extension_ui_response", id: u.id, value },
		);
		chat.ui = chat.ui.filter((x) => x.id !== u.id);
	}

	function onwindowkey(e: KeyboardEvent) {
		if (focused && primaryKey(e) && e.key === "f") {
			searchOpen = !searchOpen;
			if (!searchOpen) search = "";
			e.preventDefault();
		}
	}
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
	<Transcript bind:this={transcript} {chat} items={shown} onanswer={answer} />
	<Composer
		bind:draft={() => tab.draft ?? "", (v) => (tab.draft = v)}
		bind:images
		commands={chat.commands}
		files={atFiles}
		busy={chat.busy}
		{focused}
		onsubmit={() => submit()}
		onabort={() => send({ type: "abort" })}
		onhistory={stepHistory}
	/>
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
	.dim {
		color: var(--ca-text-secondary);
		font-size: 11.5px;
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
</style>
