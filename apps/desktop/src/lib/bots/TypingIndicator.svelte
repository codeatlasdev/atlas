<script lang="ts">
	import { typingLabel } from "#lib/bots/botFace.ts";
	import BotFace from "#lib/bots/BotFace.svelte";
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
	const faceBots = $derived(bots.slice(0, 2));
</script>

{#if bots.length > 0}
	<div class="typing-wrap" class:paused>
		<div class="typing-faces">
			{#each faceBots as b, i (b.id)}
				<div class="face-slot" style:margin-left={i > 0 ? "-6px" : "0"}>
					<BotFace seed={b.id} state="working" size={20} />
				</div>
			{/each}
		</div>
		<span class="typing-text">{label}</span>
	</div>
{/if}

<style>
	.typing-wrap {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 4px 12px 4px 8px;
	}

	.typing-faces {
		display: flex;
		align-items: center;
	}

	.face-slot {
		display: flex;
		align-items: center;
	}

	.typing-text {
		background: linear-gradient(
			90deg,
			var(--ca-text-secondary) 0%,
			var(--ca-text-primary) 40%,
			var(--ca-text-primary) 60%,
			var(--ca-text-secondary) 100%
		);
		background-size: 200% 100%;
		background-clip: text;
		-webkit-background-clip: text;
		color: transparent;
		animation: shimmer 2s ease-in-out infinite;
		font-size: 12px;
	}

	@keyframes shimmer {
		0% {
			background-position: 200% 0;
		}
		100% {
			background-position: -200% 0;
		}
	}

	.paused .typing-text {
		animation-play-state: paused;
	}

	@media (prefers-reduced-motion: reduce) {
		.typing-text {
			animation: none;
			color: var(--ca-text-secondary);
			background: none;
			-webkit-background-clip: initial;
			background-clip: initial;
		}
	}
</style>
