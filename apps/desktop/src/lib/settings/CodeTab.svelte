<script lang="ts">
	import { Switch } from "@codeatlasdev/ui";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { updates } from "#lib/stores/updates.svelte.ts";
	import EditorRows from "./EditorRows.svelte";
	import FontRows from "./FontRows.svelte";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import { savePref } from "./savePref.ts";
	import { m } from "#lib/i18n/m.ts";

	let p = $derived(prefs.value);
</script>

<Group title={m.settings_editor_code_tab_code_font()}>
	<FontRows
		idPrefix="buffer"
		mono
		familyLabel={m.settings_editor_code_tab_code_font()}
		familyDescription={m.settings_editor_code_tab_code_font_hint()}
		sizeLabel={m.settings_editor_code_tab_code_font_size()}
		sizeDescription={m.settings_editor_code_tab_code_font_size_hint()}
		family={p.buffer_font_family}
		size={p.buffer_font_size}
		onfamily={(v) => savePref("buffer_font_family", v)}
		onsize={(v) => savePref("buffer_font_size", v)}
	>
		{#snippet extra()}
			<Row
				id="buffer-line-height"
				label={m.settings_editor_code_tab_line_spacing()}
				description={m.settings_editor_code_tab_space_between_lines_as_a()}
			>
				<NumInput
					label={m.settings_editor_code_tab_line_spacing()}
					unit="×"
					value={p.buffer_line_height}
					onchange={(v) => savePref("buffer_line_height", v)}
				/>
			</Row>
		{/snippet}
		{#snippet preview()}
			<pre
				class="sample"
				style="font-family: {p.buffer_font_family}; font-size: {p.buffer_font_size}px; line-height: {p.buffer_line_height}">const answer = items.filter((x) =&gt; x.ok).length;
if (answer !== 42) throw new Error("not yet");</pre>
		{/snippet}
	</FontRows>
</Group>

<EditorRows />

<Group
	title={m.settings_editor_code_tab_language_servers()}
	description={m.settings_editor_code_tab_which_servers_are_installed_custom()}
>
	<Row
		id="lsp-enable"
		label={m.settings_editor_code_tab_language_servers()}
		description={m.settings_editor_code_tab_completion_diagnostics_hover_and_go()}
	>
		<Switch
			aria-label={m.settings_editor_code_tab_language_servers()}
			checked={p.enable_language_servers}
			onchange={(v) => savePref("enable_language_servers", v)}
		/>
	</Row>
	<Row
		id="lsp-format-on-save"
		label={m.settings_editor_code_tab_format_on_save()}
		description={m.settings_editor_code_tab_format_the_file_with_its()}
	>
		<Switch
			aria-label={m.settings_editor_code_tab_format_on_save()}
			checked={p.format_on_save}
			onchange={(v) => savePref("format_on_save", v)}
		/>
	</Row>
	<Row
		id="lsp-inlay-hints"
		label={m.settings_editor_code_tab_inlay_hints()}
		description={m.settings_editor_code_tab_show_inferred_types_and_parameter()}
	>
		<Switch
			aria-label={m.settings_editor_code_tab_inlay_hints()}
			checked={p.inlay_hints.enabled}
			onchange={(v) => savePref("inlay_hints", { enabled: v })}
		/>
	</Row>
	<Row
		id="lsp-manage"
		label={m.settings_editor_code_tab_manage_language_servers()}
		description={m.settings_editor_code_tab_install_update_or_replace_the()}
	>
		<button class="link" onclick={() => updates.show("editor/languages")}>
			{m.settings_editor_code_tab_open_languages()}
		</button>
	</Row>
</Group>

<style>
	.link {
		border: 0;
		background: none;
		padding: 0;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.link:hover {
		color: var(--ca-text-primary);
	}
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
