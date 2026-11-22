<script lang="ts">
	/**
	 * Native chat for an omp pipe-mode agent: reads the pty's JSONL stream into `ChatState`
	 * (see `rpc.ts`) and writes commands back. The transcript and composer are in `chat/`.
	 */
	import { usePoll } from "#lib/clock.svelte.ts";
	import { onMount } from "svelte";
	import { primaryKey } from "#lib/keys.ts";
	import { toast } from "@codeatlasdev/ui";
	import { SquareTerminal } from "@lucide/svelte";
	import { host } from "#lib/host.ts";
	import { openPtyStream } from "#lib/ptyChannel.ts";
	import { ptyStream } from "#lib/ptyStream.ts";
	import { hostEvents } from "#lib/hostEventsApp.ts";
	import { errText } from "#lib/api.ts";
	import { apply, emptyChat, LineBuffer, type ChatState, type UiRequest } from "#lib/rpc.ts";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { worktreeFiles } from "#lib/stores/files.svelte.ts";
	import Composer, { type Pasted } from "./chat/Composer.svelte";
	import { pushHistory } from "./chat/composer.ts";
	import Transcript from "./chat/Transcript.svelte";
	import { m } from "#lib/i18n/m.ts";
	import { formatNumber } from "#lib/i18n/fmt.ts";

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
	}

	const poll = () => send({ type: "get_state", id: "state" });
	usePoll(STATE_POLL_MS, () => focused, poll);

	onMount(() => {
		const offs: (() => void)[] = [];
		// History first, then live output (see ptyStream.ts); the reset of a replay is `feed`'s own.
		const stream = tab.pty
			? ptyStream({
					open: (f) => openPtyStream(tab.pty!, f),
					attached: async () => {},
					write: (b, replay, done) => {
						feed(b, replay);
						done();
					},
					reset: () => {},
				})
			: undefined;
		if (stream) {
			offs.push(() => stream.dispose());
			offs.push(hostEvents.onReconnected(() => stream.reattach()));
			// hostd dropped this view for not reading (a hidden webview); the replay redraws the transcript.
			offs.push(hostEvents.onDropped(tab.pty!, () => stream.reattach()));
		}
		window.setTimeout(poll, 1500);
		void worktreeFiles(tab.worktreeId).then((f) => (atFiles = f));
		return () => {
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
		{#if chat.contextPercent !== null}<span class="dim"
				>{m.workbench_chat_context({
					percent: formatNumber(chat.contextPercent / 100, { style: "percent", maximumFractionDigits: 0 }),
				})}</span
			>{/if}
		{#if chat.status}<span class="dim">{chat.status}</span>{/if}
		<span class="spacer"></span>
		<button title={m.workbench_chat_compact_title()} onclick={() => send({ type: "compact", id: "compact" })}
			>{m.workbench_chat_compact()}</button
		>
		<button title={m.workbench_chat_raw_protocol()} onclick={() => (tab.rawView = true)}
			><SquareTerminal size={13} /></button
		>
	</div>
	{#if searchOpen}
		<!-- svelte-ignore a11y_autofocus -->
		<input class="search" autofocus placeholder={m.workbench_chat_search_transcript()} bind:value={search} />
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
