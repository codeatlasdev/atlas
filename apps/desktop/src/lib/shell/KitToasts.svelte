<script lang="ts">
	/**
	 * Draws the kit's toast queue (`toast.success(…)` and friends, 94 call sites) as corner cards instead of the
	 * kit's own `Toaster`, which stacks itself and knows nothing about the other cards. The kit keeps the queue,
	 * dedupe and timers; the corner decides where each one goes. Errors and warnings are `alert` cards: they outrank
	 * plain toasts and are announced at once.
	 */
	import { Button, toast, type ToastItem } from "@codeatlasdev/ui";
	import CornerNotice from "./CornerNotice.svelte";

	const TONE = {
		error: "error",
		warning: "warning",
		success: "success",
		info: "info",
		default: "neutral",
	} as const;

	function run(t: ToastItem) {
		t.action?.onClick();
		toast.remove(t.id);
	}
</script>

{#each toast.items as t (t.id)}
	{#snippet act()}
		{#if t.action}<Button size="sm" variant="glass" onclick={() => run(t)}>{t.action.label}</Button>{/if}
	{/snippet}
	<CornerNotice
		id="toast:{t.id}"
		kind={t.variant === "error" || t.variant === "warning" ? "alert" : "toast"}
		tone={TONE[t.variant]}
		title={t.message}
		body={t.description}
		count={t.count}
		actions={t.action ? act : undefined}
		ondismiss={() => toast.remove(t.id)}
		onpause={() => toast.pause(t.id)}
		onresume={() => toast.resume(t.id)}
	/>
{/each}
