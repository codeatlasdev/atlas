<script lang="ts">
	import { Spinner } from "@codeatlasdev/ui";
	import { Check } from "@lucide/svelte";
	import type { AgentState } from "#lib/stores/agents.svelte.ts";

	let { state }: { state: AgentState | null } = $props();
</script>

{#if state}
	<span class="g {state}" title={state}>
		{#if state === "working"}<Spinner
				size={11}
			/>{:else if state === "waiting" || state === "blocked"}?{:else if state === "done"}<Check
				size={11}
			/>{:else}•{/if}
	</span>
{/if}

<style>
	.g {
		width: 14px;
		flex-shrink: 0;
		display: inline-flex;
		justify-content: center;
		font-size: 11px;
		font-weight: 800;
		color: var(--ca-text-disabled);
	}
	.waiting,
	.blocked {
		color: var(--ca-warning-main, #d99a00);
	}
	.done {
		color: var(--ca-success-main, #2a9d4b);
	}
	.working {
		color: var(--ca-brand-main);
	}
</style>
