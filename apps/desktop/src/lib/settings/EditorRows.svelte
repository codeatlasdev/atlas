<script lang="ts">
	import { SegmentedControl, Switch } from "@codeatlasdev/ui";
	import type { EditorPrefs } from "#lib/prefs.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import { savePref } from "./savePref.ts";
	import { formatRulers, parseRulers } from "./rulers.ts";
	import TextInput from "./TextInput.svelte";
	import { m } from "#lib/i18n/m.ts";

	let e = $derived(prefs.value.editor);
	const save = (patch: Partial<EditorPrefs>) => savePref("editor", { ...e, ...patch });
</script>

<Group title={m.settings_editor_rows_editing()}>
	<Row id="editor-tab-size" label={m.settings_editor_rows_tab_size()} description={m.settings_editor_rows_columns_a_tab_or_one()}>
		<NumInput
			label={m.settings_editor_rows_tab_size()}
			unit={m.settings_editor_rows_unit_columns()}
			value={e.tab_size}
			onchange={(v) => save({ tab_size: v })}
		/>
	</Row>
	<Row
		id="editor-insert-spaces"
		label={m.settings_editor_rows_indent_with_spaces()}
		description={m.settings_editor_rows_pressing_tab_inserts_spaces_turn()}
	>
		<Switch
			aria-label={m.settings_editor_rows_indent_with_spaces()}
			checked={e.insert_spaces}
			onchange={(v) => save({ insert_spaces: v })}
		/>
	</Row>
	<Row
		id="editor-trim-whitespace"
		label={m.settings_editor_rows_trim_trailing_whitespace_on_save()}
		description={m.settings_editor_rows_remove_spaces_and_tabs_at()}
	>
		<Switch
			aria-label={m.settings_editor_rows_trim_trailing_whitespace_on_save()}
			checked={e.trim_trailing_whitespace_on_save}
			onchange={(v) => save({ trim_trailing_whitespace_on_save: v })}
		/>
	</Row>
	<Row
		id="editor-final-newline"
		label={m.settings_editor_rows_add_a_final_newline_on()}
		description={m.settings_editor_rows_end_every_file_with_exactly()}
	>
		<Switch
			aria-label={m.settings_editor_rows_add_a_final_newline_on()}
			checked={e.insert_final_newline_on_save}
			onchange={(v) => save({ insert_final_newline_on_save: v })}
		/>
	</Row>
	<Row
		id="editor-auto-reload"
		label={m.settings_editor_rows_reload_files_changed_on_disk()}
		description={m.settings_editor_rows_update_an_open_file_when()}
	>
		<Switch
			aria-label={m.settings_editor_rows_reload_files_changed_on_disk()}
			checked={e.auto_reload}
			onchange={(v) => save({ auto_reload: v })}
		/>
	</Row>
	<Row
		id="editor-large-file"
		label={m.settings_editor_rows_large_file_limit()}
		description={m.settings_editor_rows_files_bigger_than_this_open()}
	>
		<NumInput
			label={m.settings_editor_rows_large_file_limit()}
			unit="MB"
			value={e.large_file_threshold_bytes / 1_000_000}
			onchange={(v) => save({ large_file_threshold_bytes: Math.round(v * 1_000_000) })}
		/>
	</Row>
</Group>

<Group title={m.settings_editor_rows_display()}>
	<Row id="editor-word-wrap" label={m.settings_editor_rows_word_wrap()} description={m.settings_editor_rows_wrap_long_lines_at_the()}>
		<Switch aria-label={m.settings_editor_rows_word_wrap()} checked={e.word_wrap} onchange={(v) => save({ word_wrap: v })} />
	</Row>
	<Row id="editor-line-numbers" label={m.settings_editor_rows_line_numbers()} description={m.settings_editor_rows_show_line_numbers_in_the()}>
		<Switch aria-label={m.settings_editor_rows_line_numbers()} checked={e.line_numbers} onchange={(v) => save({ line_numbers: v })} />
	</Row>
	<Row
		id="editor-highlight-line"
		label={m.settings_editor_rows_highlight_the_current_line()}
		description={m.settings_editor_rows_tint_the_line_the_cursor()}
	>
		<Switch
			aria-label={m.settings_editor_rows_highlight_the_current_line()}
			checked={e.highlight_active_line}
			onchange={(v) => save({ highlight_active_line: v })}
		/>
	</Row>
	<Row id="editor-whitespace" label={m.settings_editor_rows_show_whitespace()} description={m.settings_editor_rows_draw_dots_for_spaces_and()}>
		<SegmentedControl
			aria-label={m.settings_editor_rows_show_whitespace()}
			size="sm"
			value={e.render_whitespace}
			items={[
				{ value: "none", label: m.settings_editor_rows_whitespace_never() },
				{ value: "selection", label: m.settings_editor_rows_whitespace_selection() },
				{ value: "all", label: m.settings_editor_rows_whitespace_always() },
			]}
			onchange={(v) => save({ render_whitespace: v as EditorPrefs["render_whitespace"] })}
		/>
	</Row>
	<Row
		id="editor-indent-guides"
		label={m.settings_editor_rows_indent_guides()}
		description={m.settings_editor_rows_draw_a_faint_vertical_line()}
	>
		<Switch
			aria-label={m.settings_editor_rows_indent_guides()}
			checked={e.indent_guides}
			onchange={(v) => save({ indent_guides: v })}
		/>
	</Row>
	<Row
		id="editor-bracket-colors"
		label={m.settings_editor_rows_color_matching_brackets()}
		description={m.settings_editor_rows_give_each_level_of_nested()}
	>
		<Switch
			aria-label={m.settings_editor_rows_color_matching_brackets()}
			checked={e.bracket_pair_colorization}
			onchange={(v) => save({ bracket_pair_colorization: v })}
		/>
	</Row>
	<Row
		id="editor-sticky-scroll"
		label={m.settings_editor_rows_sticky_scroll()}
		description={m.settings_editor_rows_keep_the_enclosing_function_or()}
	>
		<Switch
			aria-label={m.settings_editor_rows_sticky_scroll()}
			checked={e.sticky_scroll}
			onchange={(v) => save({ sticky_scroll: v })}
		/>
	</Row>
	<Row id="editor-minimap" label={m.settings_editor_rows_minimap()} description={m.settings_editor_rows_show_a_small_overview_of()}>
		<Switch aria-label={m.settings_editor_rows_minimap()} checked={e.minimap} onchange={(v) => save({ minimap: v })} />
	</Row>
	<Row
		id="editor-rulers"
		label={m.settings_editor_rows_rulers()}
		description={m.settings_editor_rows_columns_where_a_vertical_line()}
	>
		<TextInput
			mono
			width="120px"
			label={m.settings_editor_rows_rulers()}
			placeholder="80, 100"
			value={formatRulers(e.rulers)}
			onchange={(v) => save({ rulers: parseRulers(v) })}
		/>
	</Row>
	<Row
		id="editor-scroll-past-end"
		label={m.settings_editor_rows_scroll_past_the_last_line()}
		description={m.settings_editor_rows_let_the_last_line_scroll()}
	>
		<Switch
			aria-label={m.settings_editor_rows_scroll_past_the_last_line()}
			checked={e.scroll_past_end}
			onchange={(v) => save({ scroll_past_end: v })}
		/>
	</Row>
	<Row
		id="editor-smooth-scroll"
		label={m.settings_editor_rows_smooth_scrolling()}
		description={m.settings_editor_rows_animate_scrolling_when_you_jump()}
	>
		<Switch
			aria-label={m.settings_editor_rows_smooth_scrolling()}
			checked={e.smooth_scrolling}
			onchange={(v) => save({ smooth_scrolling: v })}
		/>
	</Row>
</Group>
