<script lang="ts">
	/** Chat composer: slash/@file suggestions, image chips, attach button, textarea, stop button. */
	import { Paperclip, Square, X } from "@lucide/svelte";
	import { b64encode } from "#lib/host.ts";
	import type { ChatState } from "#lib/rpc.ts";
	import ChatTextarea from "./ChatTextarea.svelte";
	import { atHits as findAtHits, completeAt, slashHits } from "./composer.ts";
	import { m } from "#lib/i18n/m.ts";
	import { kbd } from "#lib/keys.ts";

	export interface Pasted {
		name: string;
		data: string;
		mimeType: string;
	}

	let {
		draft = $bindable(""),
		images = $bindable(),
		commands,
		files,
		busy,
		focused,
		onsubmit,
		onabort,
		onhistory,
	}: {
		draft: string;
		images: Pasted[];
		commands: ChatState["commands"];
		/** Worktree files offered after `@`. */
		files: string[];
		busy: boolean;
		focused: boolean;
		onsubmit: () => void;
		onabort: () => void;
		/** ↑ / ↓ in the box: the parent owns the prompt history and sets `draft`. */
		onhistory: (dir: "up" | "down") => boolean;
	} = $props();

	let input: HTMLTextAreaElement | undefined = $state();

	let slash = $derived(slashHits(draft, commands));
	let atHits = $derived(findAtHits(draft, files));

	$effect(() => {
		if (focused) input?.focus();
	});

	async function attach(list: FileList | null) {
		for (const f of list ?? []) {
			if (!f.type.startsWith("image/")) continue;
			images.push({ name: f.name, mimeType: f.type, data: b64encode(new Uint8Array(await f.arrayBuffer())) });
		}
	}

	function onkeydown(e: KeyboardEvent) {
		if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
			e.preventDefault();
			if (slash.length && draft !== `/${slash[0].name}`) draft = `/${slash[0].name} `;
			else if (atHits.length) draft = completeAt(draft, atHits[0]);
			else onsubmit();
		} else if (e.key === "ArrowUp" && !draft.includes("\n")) {
			if (onhistory("up")) e.preventDefault();
		} else if (e.key === "ArrowDown") {
			if (onhistory("down")) e.preventDefault();
		} else if (e.key === "Escape" && busy) onabort();
	}
</script>

<div class="composer">
	{#if slash.length || atHits.length}
		<div class="suggest">
			{#each slash as c (c.name)}
				<button onclick={() => (draft = `/${c.name} `)}
					><b>/{c.name}</b> <span class="dim">{c.description}</span></button
				>
			{/each}
			{#each atHits as f (f)}
				<button onclick={() => (draft = completeAt(draft, f))}>@{f}</button>
			{/each}
		</div>
	{/if}
	{#if images.length}
		<div class="imgs">
			{#each images as im, i (i)}
				<span class="chip"
					>{im.name}<button
						aria-label={m.crew_chat_composer_remove_image({ name: im.name })}
						onclick={() => images.splice(i, 1)}><X size={10} /></button
					></span
				>
			{/each}
		</div>
	{/if}
	<div class="row">
		<label class="attach" title={m.workbench_chat_attach_image()}>
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
			value={draft}
			oninput={(e) => (draft = (e.currentTarget as HTMLTextAreaElement).value)}
			{onkeydown}
			placeholder={busy
				? m.workbench_chat_composer_busy_placeholder({ key: kbd("escape") })
				: m.workbench_chat_composer_placeholder()}></textarea>
		{#if busy}
			<button class="stop" title={m.workbench_chat_stop({ key: kbd("escape") })} onclick={onabort}
				><Square size={12} fill="currentColor" /></button
			>
		{/if}
	</div>
</div>

<style>
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
	.dim {
		color: var(--ca-text-secondary);
		font-size: 11.5px;
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
	/* The field takes the row: at its intrinsic width (~160px) a longer placeholder, in any language, was cut off. */
	.row textarea {
		flex: 1;
		min-width: 0;
		resize: none;
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
</style>
