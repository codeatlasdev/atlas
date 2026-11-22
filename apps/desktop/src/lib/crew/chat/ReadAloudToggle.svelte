<script lang="ts">
	/**
	 * ReadAloudToggle — under the message box: the per-device "Read replies aloud" switch (off by default) and, while a
	 * reply is being spoken, the "Speaking" indicator with its stop button. "Speaking" is about the reply the shell is
	 * reading; the microphone's "Microphone is on" lives with the microphone and is never the same thing.
	 */
	import { Square, Volume2 } from "@lucide/svelte";
	import type { ChatState } from "./chat.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		chat: ChatState;
	}

	let { chat }: Props = $props();

	const speech = $derived(chat.speech);
	const on = $derived(chat.readsAloud());
	const unavailable = $derived(speech.available === false);
</script>

<span class="read-aloud">
	<button
		type="button"
		class="toggle"
		class:on
		role="switch"
		aria-checked={on}
		data-read-aloud
		disabled={unavailable}
		title={unavailable ? m.crew_voice_read_aloud_unavailable() : m.crew_voice_read_aloud_tip()}
		onclick={() => void chat.setReadAloud(!on)}
	>
		<Volume2 size={13} aria-hidden="true" />
		{m.crew_voice_read_aloud()}
	</button>
	{#if unavailable}
		<span class="note" data-read-aloud-unavailable>{m.crew_voice_read_aloud_unavailable()}</span>
	{/if}
	{#if speech.speaking}
		<span class="speaking" data-speaking>
			<span class="bars" aria-hidden="true"><i></i><i></i><i></i></span>
			{m.crew_voice_speaking()}
			<button
				type="button"
				class="stop"
				aria-label={m.crew_voice_speaking_stop()}
				title={m.crew_voice_speaking_stop()}
				onclick={() => speech.stop()}><Square size={10} aria-hidden="true" /></button
			>
		</span>
	{/if}
	{#if speech.error}
		<span class="note" role="alert">{speech.error}</span>
	{/if}
	<span class="sr-only" role="status" aria-live="polite">{speech.announce}</span>
</span>

<style>
	.read-aloud {
		display: inline-flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 10px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 8px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: inherit;
		font: inherit;
		cursor: default;
	}
	.toggle:hover:not(:disabled) {
		color: var(--ca-text-primary);
	}
	.toggle.on {
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 18%, transparent);
		border-color: color-mix(in oklab, var(--crew, var(--ca-brand-main)) 50%, transparent);
	}
	.toggle:focus-visible,
	.stop:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
		outline-offset: 1px;
	}
	.toggle:disabled {
		opacity: 0.55;
	}
	.speaking {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--ca-text-primary);
	}
	.bars {
		display: inline-flex;
		align-items: flex-end;
		gap: 2px;
		height: 10px;
	}
	.bars i {
		width: 2px;
		height: 10px;
		transform: scaleY(0.6);
		transform-origin: bottom;
		background: currentColor;
		border-radius: 1px;
	}
	@media (prefers-reduced-motion: no-preference) {
		.bars i {
			animation: bar 0.9s ease-in-out infinite;
		}
		.bars i:nth-child(2) {
			animation-delay: 0.15s;
		}
		.bars i:nth-child(3) {
			animation-delay: 0.3s;
		}
	}
	@keyframes bar {
		50% {
			transform: scaleY(1);
		}
	}
	.stop {
		width: 20px;
		height: 20px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		border-radius: 50%;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: inherit;
		cursor: default;
	}
	.note {
		max-width: 360px;
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
