<script lang="ts">
	// Test host for the chat Transcript: hands the test the reactive ChatState the real ChatTab owns.
	import Transcript from "#lib/components/chat/Transcript.svelte";
	import { emptyChat, type ChatState } from "#lib/rpc.ts";

	let { expose }: { expose: (chat: () => ChatState) => void } = $props();
	let chat = $state<ChatState>(emptyChat());
	expose(() => chat);
</script>

<Transcript {chat} items={chat.items} onanswer={() => {}} />
