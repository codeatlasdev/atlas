<script lang="ts">
	import { Chip, EmptyState, Spinner } from "@codeatlasdev/ui";
	import { ScrollText } from "@lucide/svelte";
	import { m } from "#lib/i18n/m.ts";

	let {
		loading,
		stopped,
		filtering,
		filtered,
		rangeLabel,
		onreset,
	}: {
		/** The stack runs but its first batch of lines has not arrived. */
		loading: boolean;
		/** Stack stopped and nothing buffered. */
		stopped: boolean;
		filtering: boolean;
		/** Any filter (search, service, level, range) is active. */
		filtered: boolean;
		/** Label of the active time range, or null when there is none. */
		rangeLabel: string | null;
		onreset: () => void;
	} = $props();
</script>

<div class="none">
	{#if loading}
		<Spinner size={20} label={m.logs_empty_loading_logs()} />
	{:else if stopped}
		<EmptyState
			icon={ScrollText}
			title={m.logs_empty_stack_is_stopped()}
			description={m.logs_empty_logs_stream_here_once_it()}
		/>
	{:else if filtering}
		<Spinner size={20} label={m.logs_empty_filtering()} />
	{:else if filtered}
		<EmptyState
			icon={ScrollText}
			title={m.logs_empty_no_lines_match()}
			description={rangeLabel
				? m.logs_empty_nothing_in_range({ range: rangeLabel })
				: m.logs_empty_clear_search_filters()}
		>
			{#snippet actions()}
				<Chip size="sm" label={m.logs_empty_reset_filters()} onclick={onreset} />
			{/snippet}
		</EmptyState>
	{:else}
		<EmptyState icon={ScrollText} title={m.logs_empty_waiting_for_output()} />
	{/if}
</div>

<style>
	.none {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--ca-text-secondary);
		z-index: 1;
		user-select: none;
		-webkit-user-select: none;
		cursor: default;
	}
</style>
