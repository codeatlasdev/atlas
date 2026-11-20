<script lang="ts">
	// Test host for the message box with a microphone: owns the draft the way the chat does (`bind:value`) and reports
	// the draft and the sends to the test.
	import Composer from "#lib/crew/chat/Composer.svelte";
	import type { Dictation } from "#lib/crew/voice/dictation.svelte.ts";

	let {
		initial = "",
		dictation,
		unsupported = false,
		ondraft,
		onsend,
	}: {
		initial?: string;
		dictation: Dictation;
		unsupported?: boolean;
		ondraft: (value: string) => void;
		onsend: () => void;
	} = $props();

	// svelte-ignore state_referenced_locally
	let value = $state(initial);
	$effect(() => ondraft(value));
</script>

<Composer
	bind:value
	placeholder="Message"
	label="Task prompt"
	{dictation}
	dictationUnsupported={unsupported}
	{onsend}
/>
