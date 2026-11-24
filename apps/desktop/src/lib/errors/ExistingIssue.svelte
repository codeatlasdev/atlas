<script lang="ts">
	/**
	 * "Report issue" found an open issue with this error's fingerprint (#320): offer to add to it before a new one.
	 * Loaded on demand by `ErrorPanel`, so the startup bundle does not carry it.
	 */
	import { Button } from "@codeatlasdev/ui";
	import { Bug } from "@lucide/svelte";
	import { m } from "#lib/i18n/m.ts";
	import type { FoundIssue } from "./issueLookup.ts";

	let { issue, meToo, reportNew }: { issue: FoundIssue; meToo: () => void; reportNew: () => void } = $props();
</script>

<div class="found" role="note">
	<span class="bug" aria-hidden="true"><Bug size={14} /></span>
	<div>
		<b>{m.errors_panel_already_reported({ number: issue.number, title: issue.title })}</b>
		<p>{m.errors_panel_me_too_hint()}</p>
		<div class="actions">
			<Button size="sm" variant="filled" onclick={meToo}>{m.errors_panel_me_too()}</Button>
			<Button size="sm" variant="ghost" onclick={reportNew}>{m.errors_panel_report_anyway()}</Button>
		</div>
	</div>
</div>

<style>
	.found {
		display: flex;
		gap: 9px;
		padding: 10px 12px;
		border-radius: 9px;
		font-size: 12.5px;
		line-height: 1.45;
		background: color-mix(in oklab, var(--ca-accent, currentColor) 8%, transparent);
		box-shadow: inset 0 0 0 0.5px var(--ca-outline-subtle);
	}
	p {
		margin: 2px 0 8px;
		color: var(--ca-text-secondary);
	}
	.bug {
		flex: none;
		padding-top: 1px;
		color: var(--ca-accent, currentColor);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
</style>
