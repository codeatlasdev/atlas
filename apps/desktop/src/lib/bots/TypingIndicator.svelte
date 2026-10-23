<script lang="ts">
	import BotTyping from "#lib/bots/BotTyping.svelte";
	import { typingLabel } from "#lib/bots/botFace.ts";
	import { onMount } from "svelte";

	let {
		bots,
	}: {
		bots: Array<{ id: string; name: string }>;
	} = $props();

	let paused = $state(false);

	onMount(() => {
		function onVisibility() {
			paused = document.hidden;
		}
		document.addEventListener("visibilitychange", onVisibility);
		return () => document.removeEventListener("visibilitychange", onVisibility);
	});

	const label = $derived(typingLabel(bots.map((b) => b.name)));
	const first = $derived(bots[0]);
</script>

{#if first}
	<div class="typing-wrap">
		<BotTyping seed={first.id} {label} {paused} />
	</div>
{/if}

<style>
	.typing-wrap {
		padding: 4px 40px 4px 24px;
	}
</style>
