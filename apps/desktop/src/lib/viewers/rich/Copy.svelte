<script lang="ts">
	import { Check, Copy, KeyRound } from "@lucide/svelte";
	import { m } from "#lib/i18n/m.ts";

	/** Icon button that copies `text` and shows a check for a moment. */
	let { text, label, key = false }: { text: string; label?: string; key?: boolean } = $props();
	const name = $derived(label ?? m.common_action_copy());
	let done = $state(false);
	let timer: number | undefined;

	async function copy(e: MouseEvent) {
		e.stopPropagation();
		await navigator.clipboard.writeText(text);
		done = true;
		clearTimeout(timer);
		timer = window.setTimeout(() => (done = false), 1200);
	}
</script>

<button
	class="rv-btn icon"
	title={done ? m.viewers_files_copy_copied() : name}
	aria-label={name}
	onclick={copy}
>
	{#if done}<Check size={12} />{:else if key}<KeyRound size={12} />{:else}<Copy size={12} />{/if}
</button>
