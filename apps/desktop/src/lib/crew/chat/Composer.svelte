<script lang="ts">
	/**
	 * Composer — the rounded message box under the thread: auto-growing textarea, send button, optional "+" and
	 * queue hint. The chat uses it with the "+"; the setup conversation reuses it without.
	 */
	import { ArrowUp, Plus } from "@lucide/svelte";

	interface Props {
		value: string;
		placeholder: string;
		/** aria-label of the textarea ("Task prompt", "Setup answer"). */
		label: string;
		/** Disables the send button on top of the empty check (a send/save in flight). */
		busy?: boolean;
		/** Shows the "+" before the input when given. */
		onattach?: () => void;
		/** Tasks waiting behind the running one; a hint under the pill when above zero. */
		queued?: number;
		onsend: () => void;
	}

	let {
		value = $bindable(),
		placeholder,
		label,
		busy = false,
		onattach,
		queued = 0,
		onsend,
	}: Props = $props();

	let focused = $state(false);

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
	<div class="pill" class:focused role="group">
		{#if onattach}
			<button class="pill-add" aria-label="Attach" onclick={onattach}><Plus size={16} /></button>
		{/if}
		<textarea
			class="pill-input"
			bind:value
			rows={1}
			{placeholder}
			aria-label={label}
			onkeydown={(e) => {
				if (e.key === "Enter" && !e.shiftKey) {
					e.preventDefault();
					onsend();
				}
			}}
			{@attach autoGrow}
			onfocus={() => (focused = true)}
			onblur={() => (focused = false)}></textarea>
		<button
			class="pill-send"
			class:active={!!value.trim()}
			aria-label="Send"
			disabled={!value.trim() || busy}
			onclick={onsend}><ArrowUp size={15} /></button
		>
	</div>
	{#if queued > 0}
		<div class="queue-hint">{queued} task{queued === 1 ? "" : "s"} in queue</div>
	{/if}
</div>

<style>
	.composer {
		flex: none;
		/* Same side inset as the thread, so the pill lines up with the bubbles above it. */
		padding: 8px clamp(24px, 5%, 64px) 20px;
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
	.queue-hint {
		font-size: 11px;
		color: var(--ca-text-secondary);
		text-align: center;
		margin-top: 6px;
	}
</style>
