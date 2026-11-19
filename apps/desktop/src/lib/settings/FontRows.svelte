<script lang="ts">
	import type { Snippet } from "svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import TextInput from "./TextInput.svelte";
	import { m } from "#lib/i18n/m.ts";

	/** Font family + size rows (and a preview) shared by the code and interface font groups. */
	let {
		idPrefix,
		familyLabel,
		familyDescription,
		sizeLabel,
		sizeDescription,
		family,
		size,
		mono = false,
		onfamily,
		onsize,
		extra,
		preview,
	}: {
		/** Search ids are `<idPrefix>-font` and `<idPrefix>-size`. */
		idPrefix: string;
		familyLabel: string;
		familyDescription: string;
		sizeLabel: string;
		sizeDescription: string;
		family: string;
		size: number;
		mono?: boolean;
		onfamily: (v: string) => void;
		onsize: (v: number) => void;
		/** Rows between the size row and the preview. */
		extra?: Snippet;
		preview: Snippet;
	} = $props();
</script>

<Row id="{idPrefix}-font" label={familyLabel} description={familyDescription} stack>
	<TextInput {mono} width="100%" label={familyLabel} value={family} onchange={onfamily} />
</Row>
<Row id="{idPrefix}-size" label={sizeLabel} description={sizeDescription}>
	<NumInput label={sizeLabel} unit="px" value={size} onchange={onsize} />
</Row>
{@render extra?.()}
<Row label={m.settings_terminal_font_rows_preview()}>
	{#snippet below()}{@render preview()}{/snippet}
</Row>
