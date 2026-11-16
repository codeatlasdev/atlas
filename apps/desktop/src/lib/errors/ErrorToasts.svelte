<script lang="ts">
	/** Non-blocking toasts for errors outside any boundary, plus the dialog "Details" opens. */
	import { Button, Dialog } from "@codeatlasdev/ui";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import ErrorPanel from "./ErrorPanel.svelte";
	import { errors } from "./errors.svelte.ts";
	import { headline } from "./report.ts";

	/** Longer than the stock toast (4 s): there is something to read and act on. */
	const LIFETIME_MS = 12_000;
	/** What happened and what to do; the log tail travels with Report, not as a path here. */
	const HOST_HELP =
		"Terminals can't start or reconnect until the host is back. Restart it, or report it if this keeps happening.";

	// Reading `repeats` re-arms the timers when the same error fires again, so a live problem stays visible.
	$effect(() => {
		const timers = errors.toasts.map(
			(t) => (t.repeats, window.setTimeout(() => errors.dismiss(t.id), LIFETIME_MS)),
		);
		return () => timers.forEach(clearTimeout);
	});
</script>

{#each errors.toasts as t (t.id)}
	{#snippet act()}
		{#if t.host}
			<Button size="sm" onclick={() => errors.restartHost(t.id)}>Restart terminal host</Button>
			<Button size="sm" variant="glass" onclick={() => errors.showHost(t)}>Report</Button>
		{:else}
			<Button size="sm" variant="glass" onclick={() => errors.show(t.report, t.id)}>Details</Button>
		{/if}
	{/snippet}
	<CornerNotice
		id="error:{t.id}"
		kind="alert"
		tone="error"
		title={headline(t.report).title}
		body={t.host ? HOST_HELP : t.report.message}
		count={t.repeats + 1}
		actions={act}
		ondismiss={() => errors.dismiss(t.id)}
	/>
{/each}

<Dialog
	open={!!errors.current}
	onOpenChange={(o) => !o && (errors.current = null)}
	title="Error details"
	size="md"
>
	{#if errors.current}
		<ErrorPanel
			report={errors.current}
			variant="dialog"
			title={headline(errors.current).title}
			hint={headline(errors.current).hint}
			close={() => (errors.current = null)}
			closeLabel="Close"
		/>
	{/if}
</Dialog>
