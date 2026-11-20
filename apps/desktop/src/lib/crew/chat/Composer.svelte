<script lang="ts">
	/**
	 * Composer — the rounded message box under the thread: auto-growing textarea, send button, optional "+" (image
	 * picker; a pasted image is taken too), the pending images and the queue hint. The chat uses the images; the
	 * setup conversation reuses the box without them. With a `dictation` it also has the microphone: hold the button
	 * (or ⌘⇧M / Ctrl+Shift+M) and talk; the text lands in the box and is never sent by itself.
	 */
	import { tick, untrack, type Snippet } from "svelte";
	import { ArrowUp, Mic, Plus, Square, X } from "@lucide/svelte";
	import { MAX_ATTACHMENTS, imagesFromClipboard, type PendingImage } from "#lib/crew/attachments.ts";
	import type { Dictation, Target } from "#lib/crew/voice/dictation.svelte.ts";
	import { DICTATION_STROKE, clock, endsDictationHold, isDictationKey } from "#lib/crew/voice/key.ts";
	import { dictationKey } from "#lib/crew/voice/mode.svelte.ts";
	import { IS_MAC, label as keyLabel } from "#lib/keys.ts";
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
		/** The microphone: hold to talk, the text lands in the box. Without it there is no mic button. */
		dictation?: Dictation;
		/** The crew server cannot transcribe (it predates dictation): the microphone is off, and says why. */
		dictationUnsupported?: boolean;
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
		dictation,
		dictationUnsupported = false,
		onsend,
	}: Props = $props();

	let focused = $state(false);
	let picker: HTMLInputElement | undefined = $state();
	let box: HTMLTextAreaElement | undefined = $state();

	/** The box as dictation sees it: the draft with its caret or selection, and a way to write the result back. */
	const target: Target = {
		read: () => ({
			value,
			start: box?.selectionStart ?? value.length,
			end: box?.selectionEnd ?? value.length,
		}),
		write(next, cursor) {
			value = next;
			void tick().then(() => {
				box?.focus();
				box?.setSelectionRange(cursor, cursor);
			});
		},
	};
	/** What is holding the microphone down, so only its own release lets go. */
	let heldBy: "key" | "pointer" | null = null;

	$effect(() => {
		const d = dictation;
		if (!d) return;
		void d.attach();
		// Unmounting releases the microphone and drops what was recorded.
		return () => d.dispose();
	});
	// A typed, sent or emptied draft is the person's: the undo of a dictation ends, an empty draft is no dictation.
	$effect(() => {
		const draft = value;
		untrack(() => dictation?.draftChanged(draft));
	});

	/** Escape and the microphone chord, for the box and the mic button. True when the key was ours. */
	function dictationKeys(e: KeyboardEvent): boolean {
		const d = dictation;
		if (!d) return false;
		if (isDictationKey(e, IS_MAC)) {
			e.preventDefault();
			if (!e.repeat && !dictationUnsupported) {
				heldBy = "key";
				d.press(target);
			}
			return true;
		}
		if (e.key !== "Escape") return false;
		const ours = d.busy ? (d.cancel(), true) : d.error ? (d.clearError(), true) : d.restore();
		if (ours) {
			e.preventDefault();
			e.stopPropagation();
		}
		return ours;
	}

	function micDown(e: PointerEvent) {
		if (e.button !== 0 || dictationUnsupported || !dictation) return;
		// Keep the focus and the selection in the box; keep getting the release if it happens outside the button.
		e.preventDefault();
		(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
		heldBy = "pointer";
		dictation.press(target);
	}

	function letGo(by: "key" | "pointer") {
		if (heldBy !== by) return;
		heldBy = null;
		dictation?.letGo();
	}

	const shortcut = keyLabel(DICTATION_STROKE, IS_MAC);

	/** Fit the box to its text on every change of `value`, typed or set by code. Growing only on `input`
	 *  left the box two lines tall after a send cleared it, and after a pick or "edit" filled it. */
	function autoGrow(el: HTMLTextAreaElement) {
		void value;
		el.style.height = "auto";
		const maxH = parseInt(getComputedStyle(el).getPropertyValue("--composer-max-h") || "192");
		el.style.height = Math.min(el.scrollHeight, maxH) + "px";
	}
</script>

<svelte:window onkeyup={(e) => endsDictationHold(e) && letGo("key")} />

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
	{#if dictation && (dictation.busy || dictation.error)}
		<div class="dictation">
			{#if dictation.busy}
				<div class="dictation-row">
					{#if dictation.listening}
						<span class="mic-live" data-mic-live><span class="dot" aria-hidden="true"></span>{m.crew_voice_mic_live()}</span>
						<span class="elapsed">{clock(dictation.elapsedMs)}</span>
						<span class="meter" aria-hidden="true"><i style:transform="scaleX({dictation.level})"></i></span>
					{:else if dictation.phase === "starting"}
						<span>{m.crew_voice_starting()}</span>
					{:else}
						<span>{m.crew_voice_transcribing()}</span>
					{/if}
					<span class="engine" data-dictation-engine>{m.crew_voice_engine({ engine: dictation.engine })}</span>
				</div>
				{#if dictation.partial}
					<p class="preview" data-dictation-preview aria-hidden="true">{dictation.partial}</p>
				{/if}
				{#if dictation.listening}
					<p class="hint">{dictationKey.mode === "hold" ? m.crew_voice_hint_hold() : m.crew_voice_hint_tap()}</p>
				{/if}
			{:else if dictation.error}
				<div class="dictation-error" role="alert">
					<span>{dictation.error.text}</span>
					{#if dictation.error.action === "settings"}
						<button class="link" onclick={() => dictation.openSettings()}>{m.crew_voice_open_settings()}</button>
					{:else if dictation.error.action === "retry"}
						<button class="link" onclick={() => dictation.retry()}>{m.crew_voice_retry()}</button>
					{/if}
					<button class="link" aria-label={m.crew_voice_dismiss()} onclick={() => dictation.clearError()}><X size={11} /></button>
				</div>
			{/if}
		</div>
	{/if}
	<!-- Start, stop, auto-stop and the insertion, said once each for a screen reader. -->
	<div class="sr-only" role="status" aria-live="polite">{dictation?.announce ?? ""}</div>
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
			bind:this={box}
			rows={1}
			{placeholder}
			aria-label={label}
			data-composer={hook}
			onkeydown={(e) => {
				if (dictationKeys(e)) return;
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
		{#if dictation}
			<button
				class="pill-mic"
				class:live={dictation.listening}
				aria-label={dictationUnsupported
					? m.crew_voice_unsupported()
					: dictation.listening
						? m.crew_voice_stop()
						: m.crew_voice_dictate()}
				title={dictationUnsupported ? m.crew_voice_unsupported() : m.crew_voice_dictate_tip({ shortcut })}
				disabled={dictationUnsupported || dictation.phase === "transcribing"}
				onpointerdown={micDown}
				onpointerup={() => letGo("pointer")}
				onpointercancel={() => letGo("pointer")}
				onkeydown={(e) => void dictationKeys(e)}
				onclick={(e) => {
					// A pointer press already acted; a click with no pointer is the keyboard (Space or Enter).
					if (e.detail === 0 && dictation) dictation.toggle(target);
				}}
			>
				{#if dictation.listening}<Square size={13} />{:else}<Mic size={15} />{/if}
			</button>
		{/if}
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
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
	.pill-mic {
		flex: none;
		width: 36px;
		height: 36px;
		min-height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: none;
		border-radius: 50%;
		color: var(--ca-text-secondary);
		cursor: default;
		align-self: flex-end;
		/* Held with the pointer: no text selection, no touch panning, no long-press menu. */
		user-select: none;
		touch-action: none;
	}
	.pill-mic:hover:not(:disabled) {
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
	}
	.pill-mic:disabled {
		opacity: 0.4;
	}
	/* Red and a stop square: the state is never colour alone. */
	.pill-mic.live {
		color: var(--ca-semantic-error-contrast, #fff);
		background: var(--ca-semantic-error);
	}
	.dictation {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin: 0 6px 8px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.dictation-row {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.mic-live {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
		color: var(--ca-semantic-error);
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ca-semantic-error);
	}
	@media (prefers-reduced-motion: no-preference) {
		.dot {
			animation: mic-pulse 1.4s ease-in-out infinite;
		}
		.meter i {
			transition: transform 60ms linear;
		}
	}
	@keyframes mic-pulse {
		50% {
			opacity: 0.35;
		}
	}
	.elapsed {
		font-variant-numeric: tabular-nums;
	}
	.meter {
		flex: none;
		width: 72px;
		height: 4px;
		border-radius: 2px;
		overflow: hidden;
		background: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
	}
	.meter i {
		display: block;
		height: 100%;
		background: var(--ca-semantic-error);
		transform-origin: left center;
		transform: scaleX(0);
	}
	.engine {
		margin-left: auto;
	}
	/* The live words, before the engine has the final text: dimmed, the box itself stays what it was. */
	.preview {
		margin: 0;
		font-size: 13.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
		opacity: 0.75;
		font-style: italic;
		max-height: 4.4em;
		overflow-y: auto;
	}
	.hint {
		margin: 0;
		font-size: 11px;
		color: var(--ca-text-tertiary, var(--ca-text-secondary));
	}
	.dictation-error {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
		color: var(--ca-semantic-error);
	}
	.link {
		background: none;
		border: none;
		padding: 2px 4px;
		color: var(--ca-text-primary);
		text-decoration: underline;
		cursor: default;
		font: inherit;
	}
</style>
