<script lang="ts">
	/**
	 * VoiceMic — the microphone of an approval or choice card: click to talk, click again to send. The recorder, the
	 * upload and the engine are the message box's (a `Dictation` inside a `VoiceAnswer`); what is said is NOT put in a
	 * draft: hostd reads it and its verdict comes back as `session.message` (and a card that closes, or a note that
	 * fills). Shows the engine in use and when the microphone is live (that is not the "Speaking" indicator, which is
	 * about replies read aloud). Escape stops it and forgets what was said.
	 */
	import { Mic, Square, X } from "@lucide/svelte";
	import type { VoiceAnswer } from "#lib/crew/voice/answerSession.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	interface Props {
		session: VoiceAnswer;
		/** The crew server cannot transcribe (it predates dictation): the button is off and says why. */
		unsupported?: boolean;
		/** The card is busy with something else (an answer in flight). */
		disabled?: boolean;
	}

	let { session, unsupported = false, disabled = false }: Props = $props();

	const d = $derived(session.dictation);

	$effect(() => {
		void session.attach();
		// The card going away releases the microphone and drops what was recorded.
		return () => session.dispose();
	});

	function onkeydown(e: KeyboardEvent) {
		if (e.key !== "Escape" || !session.busy) return;
		e.preventDefault();
		e.stopPropagation();
		session.cancel();
	}
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<span class="voice" {onkeydown} role="group" aria-label={m.crew_voice_answer()}>
	<span class="voice-row">
		<button
			type="button"
			class="voice-mic"
			class:live={d.listening}
			data-voice-mic
			aria-label={unsupported
				? m.crew_voice_unsupported()
				: d.listening
					? m.crew_voice_answer_stop()
					: m.crew_voice_answer()}
			title={unsupported ? m.crew_voice_unsupported() : m.crew_voice_answer()}
			disabled={unsupported ||
				disabled ||
				session.sending ||
				d.phase === "transcribing" ||
				d.phase === "starting"}
			onclick={() => session.toggle()}
		>
			{#if d.listening}<Square size={12} aria-hidden="true" />{:else}<Mic size={14} aria-hidden="true" />{/if}
		</button>
		{#if d.listening}
			<span class="live-label" data-mic-live
				><span class="dot" aria-hidden="true"></span>{m.crew_voice_mic_live()}</span
			>
		{:else if d.phase === "starting"}
			<span class="state">{m.crew_voice_starting()}</span>
		{:else if d.phase === "transcribing"}
			<span class="state">{m.crew_voice_transcribing()}</span>
		{:else if session.sending}
			<span class="state">{m.crew_voice_answer_sending()}</span>
		{/if}
		{#if d.busy}
			<span class="engine" data-dictation-engine>{m.crew_voice_engine({ engine: d.engine })}</span>
		{/if}
	</span>
	{#if d.error}
		<span class="error" role="alert">
			<span>{d.error.text}</span>
			{#if d.error.action === "settings"}
				<button type="button" class="link" onclick={() => d.openSettings()}
					>{m.settings_capture_permission_open_settings()}</button
				>
			{:else if d.error.action === "retry"}
				<button type="button" class="link" onclick={() => d.retry()}
					>{m.crew_onboarding_connect_step_action_retry()}</button
				>
			{/if}
			<button type="button" class="link" aria-label={m.crew_voice_dismiss()} onclick={() => d.clearError()}
				><X size={11} aria-hidden="true" /></button
			>
		</span>
	{/if}
	{#if session.message}
		<span class="message" data-voice-message>{session.message}</span>
	{/if}
	<!-- Start, stop and the verdict, said once each for a screen reader. -->
	<span class="sr-only" role="status" aria-live="polite">{session.announce || d.announce}</span>
</span>

<style>
	.voice {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.voice-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
	}
	.voice-mic {
		width: 28px;
		height: 28px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 0;
		border-radius: 50%;
		border: 0.5px solid var(--ca-outline-subtle);
		background: none;
		color: var(--ca-text-secondary);
		cursor: default;
	}
	.voice-mic:hover:not(:disabled) {
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-outline-subtle) 50%, transparent);
	}
	.voice-mic:focus-visible {
		outline: 2px solid color-mix(in oklab, var(--crew, var(--ca-brand-main)) 60%, transparent);
		outline-offset: 1px;
	}
	.voice-mic:disabled {
		opacity: 0.5;
	}
	.voice-mic.live {
		color: var(--ca-semantic-danger, #d33);
		border-color: currentColor;
	}
	.live-label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--ca-semantic-danger, #d33);
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: currentColor;
	}
	@media (prefers-reduced-motion: no-preference) {
		.dot {
			animation: pulse 1.2s ease-in-out infinite;
		}
	}
	@keyframes pulse {
		50% {
			opacity: 0.35;
		}
	}
	.engine {
		margin-left: auto;
		font-size: 11px;
	}
	.error {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		color: var(--ca-semantic-danger, #d33);
	}
	.link {
		background: none;
		border: none;
		padding: 0;
		color: inherit;
		text-decoration: underline;
		font: inherit;
		cursor: default;
	}
	.message {
		color: var(--ca-text-primary);
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
	}
</style>
