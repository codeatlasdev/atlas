<script lang="ts">
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import TextInput from "./TextInput.svelte";
	import { savePref } from "./savePref.ts";

	let p = $derived(prefs.value);
</script>

<Group title="Code font">
	<Row id="buffer-font" label="Code font" description="Font for the editor and diffs." stack>
		<TextInput
			mono
			width="100%"
			label="Code font"
			value={p.buffer_font_family}
			onchange={(v) => savePref("buffer_font_family", v)}
		/>
	</Row>
	<Row id="buffer-size" label="Code font size" description="Text size in the editor and diffs.">
		<NumInput
			label="Code font size"
			unit="px"
			value={p.buffer_font_size}
			onchange={(v) => savePref("buffer_font_size", v)}
		/>
	</Row>
	<Row
		id="buffer-line-height"
		label="Line spacing"
		description="Space between lines, as a multiple of the font size. 1.5 is comfortable."
	>
		<NumInput
			label="Line spacing"
			unit="×"
			value={p.buffer_line_height}
			onchange={(v) => savePref("buffer_line_height", v)}
		/>
	</Row>
	<Row label="Preview">
		{#snippet below()}
			<pre
				class="sample"
				style="font-family: {p.buffer_font_family}; font-size: {p.buffer_font_size}px; line-height: {p.buffer_line_height}">const answer = items.filter((x) =&gt; x.ok).length;
if (answer !== 42) throw new Error("not yet");</pre>
		{/snippet}
	</Row>
</Group>

<style>
	.sample {
		margin: 0;
		padding: 10px 12px;
		border-radius: 8px;
		overflow: auto;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
	}
</style>
