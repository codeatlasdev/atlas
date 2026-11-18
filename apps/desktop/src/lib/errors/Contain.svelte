<script lang="ts">
	/**
	 * Containment: a crash inside `children` becomes an inline error card in this spot only, the rest
	 * of the window keeps working. Used around each tab body, each dock and the project panel.
	 * `<svelte:boundary>` catches render/effect errors, not event handlers (see `errors.svelte.ts`).
	 */
	import type { Snippet } from "svelte";
	import Lazy from "#lib/components/Lazy.svelte";
	import { errors } from "./errors.svelte.ts";
	import { report, type ErrorReport } from "./report.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		children,
		where,
		kind,
		file,
		title = m.shell_contain_default_title(),
		hint = m.shell_contain_default_hint(),
		close,
		closeLabel,
	}: {
		children: Snippet;
		/** What is rendering: `Editor`, `Right sidebar`… */
		where: string;
		kind?: string;
		file?: string;
		title?: string;
		hint?: string;
		close?: () => void;
		closeLabel?: string;
	} = $props();

	let version = $state<string | undefined>();
	$effect(() => void errors.version().then((v) => (version = v)));
	/** Machine facts and recent breadcrumbs, added once the card is up (never blocks showing it). */
	let enriched = $state<ErrorReport | undefined>();

	function onerror(e: unknown) {
		// The boundary swallows the error, so the window `error` event never fires: log it here.
		console.error(`[atlas] ${where}${kind ? ` (${kind})` : ""} crashed:`, e);
	}
</script>

<svelte:boundary {onerror}>
	{@render children()}
	{#snippet failed(error, reset)}
		{@const base = report(error, { where, kind, file, version })}
		{void errors.enrich(base).then((r) => (enriched = r))}
		<Lazy
			load={() => import("./ErrorPanel.svelte")}
			props={{
				report: enriched?.message === base.message ? enriched : base,
				title,
				hint,
				retry: reset,
				close,
				closeLabel,
			}}
		/>
	{/snippet}
</svelte:boundary>
