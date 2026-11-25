<script lang="ts">
	/** Non-blocking offer at the top of the tab: adopt the theme and font of ~/.config/ghostty/config. */
	import { Button, Spinner, toast } from "@codeatlasdev/ui";
	import { Ghost, X } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import type { ImportResult } from "#lib/theme/import/color.ts";
	import ImportPreview from "./ImportPreview.svelte";
	import { previewModel } from "./preview.ts";
	import { m } from "#lib/i18n/m.ts";

	let result = $state<ImportResult | null>(null);
	let failed = $state<string | null>(null);
	let showPreview = $state(false);
	let busy = $state(false);

	// Parsed once the offer is live; the config is small and read through the host.
	$effect(() => {
		if (!themes.ghostty.offered || result || failed) return;
		themes.importGhostty().then(
			(r) => (result = r),
			(e) => (failed = errText(e)),
		);
	});

	let strip = $derived(
		result ? previewModel({ style: result.style, appearance: result.appearance }).ansi : [],
	);
	let summary = $derived(
		result
			? [result.name, result.font?.family, result.font?.size ? `${result.font.size} pt` : null]
					.filter(Boolean)
					.join(" · ")
			: "",
	);

	async function use() {
		if (!result) return;
		busy = true;
		try {
			await themes.importResult(result, { applyFont: true, select: true });
			await themes.dismissGhosttyOffer();
			toast.success(m.settings_theme_toast_using({ name: result.name }));
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}
</script>

{#if themes.ghostty.offered && !failed}
	<section class="banner" aria-label={m.settings_theme_ghostty_banner_label()}>
		<div class="row">
			<Ghost size={16} aria-hidden="true" />
			<div class="text">
				<b>{m.settings_theme_ghostty_banner_title()}</b>
				{#if result}
					<span class="sum">{summary}</span>
				{:else}
					<span class="sum"><Spinner size={11} /> {m.settings_theme_ghostty_banner_reading()}</span>
				{/if}
			</div>
			{#if strip.length}
				<span class="strip" aria-hidden="true">
					{#each strip as c, i (i)}<span style:background={c}></span>{/each}
				</span>
			{/if}
			<div class="actions">
				<Button size="sm" disabled={!result || busy} loading={busy} onclick={use}>
					{m.settings_theme_use_it()}
				</Button>
				<Button
					size="sm"
					variant="outlined"
					disabled={!result}
					aria-expanded={showPreview}
					onclick={() => (showPreview = !showPreview)}
				>
					{showPreview
						? m.settings_theme_ghostty_banner_hide_preview()
						: m.settings_theme_ghostty_banner_preview()}
				</Button>
				<Button size="sm" variant="ghost" onclick={() => themes.dismissGhosttyOffer()}>
					{m.settings_theme_ghostty_banner_not_now()}
				</Button>
			</div>
			<button
				class="x"
				aria-label={m.settings_theme_ghostty_banner_dismiss()}
				onclick={() => themes.dismissGhosttyOffer()}
			>
				<X size={13} />
			</button>
		</div>
		{#if result && showPreview}
			<div class="prev"><ImportPreview {result} name={result.name} /></div>
		{/if}
	</section>
{/if}

<style>
	.banner {
		padding: 10px 12px;
		border-radius: 10px;
		background: var(--ca-surface-raised);
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			inset 3px 0 0 var(--atlas-focus-ring, var(--ca-brand-main));
	}
	.row {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.text {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-width: 160px;
		font-size: 13px;
		line-height: 1.35;
		color: var(--ca-text-primary);
	}
	.sum {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.strip {
		display: flex;
		border-radius: 5px;
		overflow: hidden;
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.strip span {
		width: 9px;
		height: 18px;
	}
	.actions {
		display: flex;
		gap: 6px;
	}
	.x {
		display: grid;
		place-items: center;
		width: 22px;
		height: 22px;
		border: 0;
		border-radius: 6px;
		color: var(--ca-text-secondary);
		background: transparent;
	}
	.x:hover {
		background: var(--ca-action-hover);
	}
	.prev {
		margin-top: 10px;
		max-width: 260px;
	}
</style>
