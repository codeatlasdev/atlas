<script lang="ts">
	import { Button } from "@codeatlasdev/ui";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import type { ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { actionOutput } from "./actionOutput.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { project }: { project: ProjectView } = $props();

	/** How long the "View output" shortcut stays after an action finished. */
	const SHOW_MS = 8000;

	let live = $derived(store.live[project.id]);
	/** Run ids dismissed or already looked at. */
	let dismissed = $state<number[]>([]);
	let now = $state(Date.now());

	// The newest finished run, while fresh and unseen. Opening the drawer counts as seen.
	let run = $derived.by(() => {
		void live?.runs.rev;
		const r = live?.runs.list.findLast((x) => x.status !== "running");
		if (!r || r.end === null || dismissed.includes(r.id)) return null;
		return now - r.end < SHOW_MS ? r : null;
	});

	$effect(() => {
		const r = run;
		if (!r || r.end === null) return;
		const left = SHOW_MS - (Date.now() - r.end);
		now = Date.now();
		const t = setTimeout(() => (now = Date.now()), left + 50);
		return () => clearTimeout(t);
	});

	$effect(() => {
		if (actionOutput.open && run) dismissed.push(run.id);
	});

	function view() {
		if (!run) return;
		actionOutput.show(project.id, run.id);
	}
</script>

<!-- A failed action needs a look (alert); a finished one is a plain toast. -->
{#if run && !actionOutput.open && store.current?.id === project.id}
	{#snippet act()}
		<Button size="sm" variant="glass" onclick={view}>{m.panel_action_result_toast_view_output()}</Button>
	{/snippet}
	<CornerNotice
		id="action-result"
		kind={run.status === "failed" ? "alert" : "toast"}
		tone={run.status === "failed" ? "error" : "success"}
		title={run.status === "ok"
			? m.panel_action_result_toast_finished({ label: run.label })
			: m.panel_action_result_toast_failed({ label: run.label })}
		actions={act}
		ondismiss={() => dismissed.push(run!.id)}
	/>
{/if}
