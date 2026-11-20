<script lang="ts">
	/**
	 * Composer — the rounded message box under the thread: auto-growing textarea, send button, optional "+" (image
	 * picker; a pasted image is taken too), the pending images and the queue hint. The chat uses the images; the
	 * setup conversation reuses the box without them.
	 */
	import type { Snippet } from "svelte";
	import { ArrowUp, Plus, X } from "@lucide/svelte";
	import { MAX_ATTACHMENTS, imagesFromClipboard, type PendingImage } from "#lib/crew/attachments.ts";
	import ImageThumb from "./ImageThumb.svelte";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		value: string;
		placeholder: string;
		/** aria-label of the textarea ("Task prompt", "Setup answer"): translated text, so code never looks it up. */
		label: string;
		/** `data-composer` of the textarea: the stable hook for code that moves the focus into it. */
		hook?: string;
		/** Disables the send button on top of the empty check (a send/save in flight). */
		busy?: boolean;
		/** Images from the "+" picker or a paste. Shows the "+" and takes pastes when given. */
		onfiles?: (files: File[]) => void;
		/** Images waiting to be sent, shown above the box. */
		images?: PendingImage[];
		onremoveimage?: (index: number) => void;
		/** Tasks waiting behind the running one; a hint under the pill when above zero. */
		queued?: number;
		/** Controls under the box, on the left (the chat's permission-mode picker). */
		footer?: Snippet;
		onsend: () => void;
	}

	let {
		value = $bindable(),
		placeholder,
		label,
		hook,
		busy = false,
		onfiles,
		images = [],
		onremoveimage,
		queued = 0,
		footer,
		onsend,
	}: Props = $props();

	let focused = $state(false);
	let picker: HTMLInputElement | undefined = $state();

	/** Fit the box to its text on every change of `value`, typed or set by code. Growing only on `input`
	 *  left the box two lines tall after a send cleared it, and after a pick or "edit" filled it. */
	function autoGrow(el: HTMLTextAreaElement) {
		void value;
		el.style.height = "auto";
		const maxH = parseInt(getComputedStyle(el).getPropertyValue("--composer-max-h") || "192");
		el.style.height = Math.min(el.scrollHeight, maxH) + "px";
	}
</script>

<div class="composer">
	{#if images.length}
		<ul class="pending" aria-label={m.crew_chat_composer_attached_images()}>
			{#each images as im, i (im.name + i)}
				<li class="pending-item">
					<ImageThumb bytes={im.bytes} mime={im.mime} alt={im.name} size={56} />
					<button
						class="pending-x"
						aria-label={m.crew_chat_composer_remove_image({ name: im.name })}
						onclick={() => onremoveimage?.(i)}><X size={11} /></button
					>
				</li>
			{/each}
		</ul>
	{/if}
	<div class="pill" class:focused role="group">
		{#if onfiles}
			<button
				class="pill-add"
				aria-label={m.crew_chat_composer_attach()}
				disabled={images.length >= MAX_ATTACHMENTS}
				onclick={() => picker?.click()}><Plus size={16} /></button
			>
			<input
				bind:this={picker}
				class="picker"
				type="file"
				accept="image/png,image/jpeg,image/webp,image/gif"
				multiple
				hidden
				aria-label={m.crew_chat_composer_attach_images()}
				onchange={(e) => {
					const input = e.currentTarget;
					onfiles(Array.from(input.files ?? []));
					// The same file picked twice in a row must fire again.
					input.value = "";
				}}
			/>
		{/if}
		<textarea
			class="pill-input"
			bind:value
			rows={1}
			{placeholder}
			aria-label={label}
			data-composer={hook}
			onkeydown={(e) => {
				if (e.key === "Enter" && !e.shiftKey) {
					e.preventDefault();
					onsend();
				}
			}}
			onpaste={(e) => {
				const files = onfiles ? imagesFromClipboard(e) : [];
				// Text pastes go on as usual; only a paste that holds an image is taken over.
				if (files.length) {
					e.preventDefault();
					onfiles?.(files);
				}
			}}
			{@attach autoGrow}
			onfocus={() => (focused = true)}
			onblur={() => (focused = false)}></textarea>
		<button
			class="pill-send"
			class:active={!!value.trim()}
			aria-label={m.crew_chat_composer_send()}
			disabled={!value.trim() || busy}
			onclick={onsend}><ArrowUp size={15} /></button
		>
	</div>
	{#if footer || queued > 0}
		<div class="foot">
			{@render footer?.()}
			{#if queued > 0}
				<div class="queue-hint">{m.crew_chat_composer_queued_tasks({ count: queued })}</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.composer {
		flex: none;
		/* Same side inset as the thread, so the pill lines up with the bubbles above it. */
		padding: 8px clamp(24px, 5%, 64px) 20px;
	}
	.pending {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		list-style: none;
		margin: 0 0 8px;
		padding: 0 6px;
	}
	.pending-item {
		position: relative;
	}
	.pending-x {
		position: absolute;
		top: -6px;
		right: -6px;
		width: 20px;
		height: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		border: 1px solid var(--ca-outline-subtle);
		background: var(--ca-surface-overlay, var(--ca-surface-raised));
		color: var(--ca-text-secondary);
		cursor: default;
		padding: 0;
	}
	.pending-x:hover {
		color: var(--ca-text-primary);
	}
	.pill-add:disabled {
		opacity: 0.4;
	}
	.pill {
		display: flex;
		align-items: flex-end;
		/* Raised, like Grok's composer: a sunken (darker than the page) pill read as a hole with faded edges. */
		background: color-mix(in oklab, var(--ca-text-primary) 6%, var(--ca-surface-raised, var(--ca-surface)));
		/* Half the one-line height: a pill with one line, a rounded box once the text wraps. 999px kept the pill shape
		   at any height and its round ends cut into the second line. */
		border-radius: 26px;
		border: 1px solid color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
		min-height: 52px;
		padding: 8px 8px 8px 6px;
		gap: 4px;
		transition: border-color 120ms;
		--composer-max-h: 192;
	}
	.pill.focused {
		border-color: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
	}
	.pill-add {
		flex: none;
		width: 36px;
		height: 36px;
		min-height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
		border: none;
		border-radius: 50%;
		color: var(--ca-text-secondary);
		cursor: default;
		align-self: flex-end;
		flex-shrink: 0;
	}
	.pill-add:hover {
		background: color-mix(in oklab, var(--ca-outline-subtle) 80%, transparent);
		color: var(--ca-text-primary);
	}
	.pill-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		outline: none;
		font-size: 15px;
		color: var(--ca-text-primary);
		padding: 6px 4px;
		line-height: 1.5;
		resize: none;
		overflow-y: auto;
		min-height: 36px;
		max-height: 192px;
		align-self: flex-end;
		font-family: inherit;
		field-sizing: content;
	}
	/* Without the "+" before it (the setup composer), the text starts at the pill's own edge: inset it so it clears the
	   round end like the "+" does in the chat composer. */
	.pill-input:first-child {
		padding-left: 14px;
	}
	.pill-input::placeholder {
		color: var(--ca-text-secondary);
	}
	.pill-send {
		flex: none;
		width: 36px;
		height: 36px;
		min-height: 36px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 25%, transparent);
		border: none;
		color: var(--ca-text-disabled, #888);
		cursor: default;
		transition:
			background 120ms,
			color 120ms;
		align-self: flex-end;
		flex-shrink: 0;
	}
	.pill-send.active {
		background: var(--crew, var(--ca-brand-main));
		color: var(--ca-surface-base);
	}
	.pill-send:disabled:not(.active) {
		opacity: 0.5;
	}
	.foot {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		margin-top: 6px;
		padding: 0 6px;
		font-size: 11.5px;
	}
	.queue-hint {
		margin-left: auto;
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: right;
	}
</style>
