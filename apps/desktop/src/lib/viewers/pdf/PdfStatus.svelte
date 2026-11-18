<script lang="ts">
	import { FileText } from "@lucide/svelte";
	import ViewerActions from "#lib/viewers/ViewerActions.svelte";
	import type { PdfErrorKind } from "#lib/viewers/pdf/doc.ts";
	import { m } from "#lib/i18n/m.ts";

	/** Opening spinner and the "can't preview" card (password / damaged / render failure). */
	let {
		status,
		errorKind,
		errorText,
		onopen,
		onreveal,
	}: {
		status: "loading" | "error";
		errorKind: PdfErrorKind;
		errorText: string;
		onopen: () => void;
		onreveal: () => void;
	} = $props();
</script>

<div class="state" aria-busy={status === "loading" ? "true" : undefined}>
	{#if status === "loading"}
		<div class="spin"></div>
		{m.viewers_files_pdf_status_opening_pdf()}
	{:else}
		<FileText size={28} />
		<h3>
			{errorKind === "password"
				? m.viewers_files_pdf_status_password()
				: errorKind === "invalid"
					? m.viewers_files_pdf_status_invalid()
					: m.viewers_files_pdf_status_other()}
		</h3>
		<p>
			{errorKind === "other" ? errorText : m.viewers_files_pdf_status_hint()}
		</p>
		<ViewerActions {onopen} {onreveal} />
	{/if}
</div>

<style>
	.state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 40px 20px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		text-align: center;
	}
	.state h3 {
		margin: 4px 0 0;
		font-size: 15px;
		font-weight: 650;
		color: var(--ca-text-primary);
	}
	.state p {
		margin: 0 0 8px;
		max-width: 420px;
	}
	.spin {
		width: 18px;
		height: 18px;
		border: 2px solid var(--ca-outline-subtle);
		border-top-color: var(--space-accent, var(--ca-brand-main));
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.spin {
			animation: none;
		}
	}
</style>
