<script lang="ts">
	/** The real column with the real Getting started card; `toast` and `suggestion` add or remove a neighbour. */
	import Checklist from "#lib/onboarding/Checklist.svelte";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import CornerStack from "#lib/shell/CornerStack.svelte";

	let withToast = $state(false);
	let withTip = $state(false);
	export const toast = (on: boolean) => (withToast = on);
	export const suggestion = (on: boolean) => (withTip = on);
</script>

<CornerStack />
<Checklist />
{#if withToast}<CornerNotice id="t" kind="toast" title="Copied 12 lines" />{/if}
{#if withTip}<CornerNotice id="tip" kind="suggestion" title="Enable Claude Code integration" />{/if}
