<script lang="ts">
	/**
	 * Credit for the default themes. Links open in the system browser through `api.openUrl`, never as an
	 * `<a target>` inside the webview. Carries no Pro colors, swatches or screenshots.
	 */
	import { Button, toast } from "@codeatlasdev/ui";
	import { ExternalLink, Heart } from "@lucide/svelte";
	import { api, errText } from "#lib/api.ts";
	import { m } from "#lib/i18n/m.ts";

	const SITE = "https://draculatheme.com";
	const ZENO = "https://github.com/zenorocha";
	const LUXONAUTA = "https://github.com/luxonauta";
	const PRO = "https://draculatheme.com/pro";

	const open = (url: string) => api.openUrl(url).catch((e) => toast.error(errText(e)));
</script>

<section class="credit" aria-label={m.settings_theme_credit_card_label()}>
	<div class="text">
		<h3 class="title">
			<Heart size={13} aria-hidden="true" />
			{m.settings_theme_credit_card_title()}
		</h3>
		<p class="links">
			<button class="link" onclick={() => open(SITE)}>draculatheme.com <ExternalLink size={11} /></button>
			<button class="link" onclick={() => open(ZENO)}>@zenorocha <ExternalLink size={11} /></button>
			<button class="link" onclick={() => open(LUXONAUTA)}>@luxonauta <ExternalLink size={11} /></button>
		</p>
		<p class="note">{m.settings_theme_credit_card_license_note()}</p>
	</div>
	<Button variant="filled" size="sm" onclick={() => open(PRO)}
		>{m.settings_theme_credit_card_get_pro()}</Button
	>
</section>

<style>
	.credit {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		margin: 12px 14px 0;
		padding: 12px 14px;
		border-radius: 12px;
		background: color-mix(in oklab, var(--ca-brand-main) 8%, var(--ca-surface-base));
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.text {
		display: flex;
		flex-direction: column;
		gap: 3px;
		min-width: 0;
	}
	.title {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.title :global(svg) {
		color: var(--ca-brand-main);
	}
	.links {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 14px;
		margin: 0;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-brand-main);
		cursor: pointer;
	}
	.link:hover {
		text-decoration: underline;
	}
	.link:focus-visible {
		outline: 2px solid var(--atlas-focus-ring, var(--ca-brand-main));
		outline-offset: 2px;
		border-radius: 4px;
	}
	.note {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
