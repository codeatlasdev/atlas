<script lang="ts">
	import { Button, Select, SegmentedControl, Switch } from "@codeatlasdev/ui";
	import { gpu, setGpuTerminal } from "#lib/stores/gpu.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import type { Prefs } from "#lib/prefs.ts";
	import { ANSI_VARS } from "#lib/ansi.ts";
	import { terminalLift } from "#lib/theme/color.ts";
	import { restartHost } from "#lib/stores/hostStatus.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import TextInput from "./TextInput.svelte";
	import { savePref } from "./savePref.ts";
	import { m } from "#lib/i18n/m.ts";
	import { kbd } from "#lib/keys.ts";

	let p = $derived(prefs.value);
	let t = $derived(p.terminal);
	const MB = 1024 * 1024;
	const save = (patch: Partial<Prefs["terminal"]>) => savePref("terminal", { ...t, ...patch });

	/** Ends every terminal: the app reconnects and starts the current host. */
	async function restart() {
		const ok = await confirmDialog(m.settings_terminal_tab_restart_host_title(), {
			description: m.settings_terminal_tab_restart_host_body(),
			confirmLabel: m.settings_host_action_restart(),
			destructive: true,
		});
		if (ok) await restartHost();
	}

	let colors = $derived(themes.terminal);
	let bg = $derived(colors?.bg ?? "var(--ca-surface-sunken)");
	let fg = $derived(colors?.fg ?? "var(--ca-text-primary)");
	let cursor = $derived(colors?.cursor ?? fg);
	let selBg = $derived(colors?.selectionBg ?? "var(--ca-surface-selected)");
	let selFg = $derived(colors?.selectionFg ?? fg);
	/** Palette slot 0–15: the terminal theme's color, else the app's `--ansi-*` variable. */
	const ansi = (i: number) => colors?.ansi16[i] ?? `var(${ANSI_VARS[i]})`;
	/** An ANSI colour as TEXT in the preview: lifted by the terminal's own minimum-contrast rule, as the terminal draws it. */
	const asText = (i: number) => {
		const c = colors?.ansi16[i];
		const surface = colors?.bg ?? themes.current?.style["terminal.background"];
		return c && typeof surface === "string"
			? terminalLift(c, surface.slice(0, 7), t.minimum_contrast)
			: ansi(i);
	};
	const ANSI_SLOTS = [0, 1, 2, 3, 4, 5, 6, 7];
</script>

<Group title={m.settings_terminal_tab_renderer()}>
	{#if gpu.offered}
		<Row
			id="gpu"
			label={m.settings_terminal_tab_fast_terminal_gpu()}
			description={m.settings_terminal_tab_draws_local_terminals_on_the()}
		>
			<Switch
				aria-label={m.settings_terminal_tab_fast_terminal_gpu()}
				checked={gpu.terminal}
				onchange={(v) => setGpuTerminal(v)}
			/>
			{#snippet below()}
				{#if gpu.terminal && gpu.hostTooOld}
					<div class="notice">
						<span>{m.settings_terminal_tab_the_running_terminal_host_is()}</span>
						<Button size="sm" variant="outlined" onclick={restart}
							>{m.settings_terminal_tab_restart_terminal_host()}</Button
						>
					</div>
				{/if}
				{#if gpu.restartNeeded}
					<div class="notice">
						<span>{m.settings_terminal_tab_restart_to_apply_fast_terminal()}</span>
					</div>
				{/if}
			{/snippet}
		</Row>
	{/if}
	<Row
		id="host-restart"
		label={m.settings_host_name()}
		description={m.settings_terminal_tab_the_background_process_that_keeps()}
	>
		<Button size="sm" variant="outlined" onclick={restart}
			>{m.settings_terminal_tab_restart_terminal_host()}</Button
		>
	</Row>
</Group>

<Group title={m.settings_terminal_tab_text()}>
	<Row
		id="term-font"
		label={m.settings_terminal_tab_terminal_font()}
		description={m.settings_terminal_tab_font_families_in_order_like()}
		stack
	>
		<TextInput
			mono
			width="100%"
			label={m.settings_terminal_tab_terminal_font()}
			value={t.font_family}
			onchange={(v) => save({ font_family: v })}
		/>
	</Row>
	<Row
		id="term-size"
		label={m.settings_terminal_tab_terminal_font_size()}
		description={m.settings_terminal_tab_text_size_in_terminals()}
	>
		<NumInput
			label={m.settings_terminal_tab_terminal_font_size()}
			unit="px"
			value={t.font_size}
			onchange={(v) => save({ font_size: v })}
		/>
	</Row>
	<Row
		id="font-thicken"
		label={m.settings_terminal_tab_thicken_glyphs()}
		description={m.settings_terminal_tab_draw_text_slightly_heavier_helps()}
	>
		<Switch
			aria-label={m.settings_terminal_tab_thicken_glyphs()}
			checked={t.font_thicken}
			onchange={(v) => save({ font_thicken: v })}
		/>
	</Row>
	{#if gpu.supported}
		<Row
			id="ligatures"
			label={m.settings_terminal_tab_ligatures()}
			description={m.settings_terminal_tab_join_characters_like_and_into()}
		>
			<Switch
				aria-label={m.settings_terminal_tab_ligatures()}
				checked={t.ligatures}
				onchange={(v) => save({ ligatures: v })}
			/>
		</Row>
		<Row
			id="contrast"
			label={m.settings_terminal_tab_minimum_contrast()}
			description={m.settings_terminal_tab_lightens_or_darkens_text_that()}
		>
			<NumInput
				label={m.settings_terminal_tab_minimum_contrast()}
				value={t.minimum_contrast}
				onchange={(v) => save({ minimum_contrast: v, minimum_contrast_chosen: true })}
			/>
		</Row>
	{/if}
</Group>

<Group title={m.settings_terminal_tab_cursor()}>
	<Row
		id="cursor-shape"
		label={m.settings_terminal_tab_cursor_shape()}
		description={m.settings_terminal_tab_how_the_cursor_looks_in()}
	>
		<div class="select">
			<Select
				size="sm"
				value={t.cursor_shape}
				options={[
					{ value: "block", label: m.settings_terminal_tab_cursor_block() },
					{ value: "bar", label: m.settings_terminal_tab_cursor_bar() },
					{ value: "underline", label: m.settings_terminal_tab_cursor_underline() },
					{ value: "hollow", label: m.settings_terminal_tab_cursor_hollow() },
				]}
				onchange={(v) => save({ cursor_shape: v as Prefs["terminal"]["cursor_shape"] })}
			/>
		</div>
	</Row>
	<Row
		id="cursor-blink"
		label={m.settings_terminal_tab_blinking_cursor()}
		description={m.settings_terminal_tab_make_the_cursor_blink()}
	>
		<Switch
			aria-label={m.settings_terminal_tab_blinking_cursor()}
			checked={t.blinking}
			onchange={(v) => save({ blinking: v })}
		/>
	</Row>
	<Row label={m.settings_terminal_font_rows_preview()}>
		{#snippet below()}
			<div
				class="term"
				style="background: {bg}; color: {fg}; font-family: {t.font_family}; font-size: {t.font_size}px; font-weight: {t.font_thicken
					? 500
					: 400}; padding: {t.padding_y}px {t.padding_x}px"
			>
				<!-- i18n-ignore: a sample prompt and command, drawn as the terminal would -->
				<div>
					<span style="color: {asText(2)}">~/atlas</span>
					<span style="color: {asText(4)}">main</span>
					$ bun run
					<span class="sel" style="background: {selBg}; color: {selFg}">dev</span>
					<span class="cur {t.cursor_shape}" class:blink={t.blinking} style="--cur: {cursor}"></span>
				</div>
				{#each [0, 8] as base (base)}
					<div class="strip">
						{#each ANSI_SLOTS as i (i)}
							<span style="background: {ansi(base + i)}"></span>
						{/each}
					</div>
				{/each}
			</div>
		{/snippet}
	</Row>
</Group>

<Group title={m.settings_terminal_tab_padding()}>
	<Row
		id="term-padding-x"
		label={m.settings_terminal_tab_horizontal_padding()}
		description={m.settings_terminal_tab_space_left_and_right_of()}
	>
		<NumInput
			label={m.settings_terminal_tab_horizontal_padding()}
			unit="pt"
			value={t.padding_x}
			onchange={(v) => save({ padding_x: v })}
		/>
	</Row>
	<Row
		id="term-padding-y"
		label={m.settings_terminal_tab_vertical_padding()}
		description={m.settings_terminal_tab_space_above_and_below_the()}
	>
		<NumInput
			label={m.settings_terminal_tab_vertical_padding()}
			unit="pt"
			value={t.padding_y}
			onchange={(v) => save({ padding_y: v })}
		/>
	</Row>
</Group>

<Group title={m.settings_terminal_tab_behavior()}>
	<Row
		id="scrollback"
		label={m.settings_terminal_tab_scrollback()}
		description={m.settings_terminal_tab_lines_of_output_each_terminal()}
	>
		<NumInput
			label={m.settings_terminal_tab_scrollback()}
			unit="lines"
			value={t.max_scroll_history_lines}
			onchange={(v) => save({ max_scroll_history_lines: v })}
		/>
	</Row>
	{#if gpu.supported}
		<Row
			id="scrollback-memory"
			label={m.settings_terminal_tab_scrollback_memory()}
			description={m.settings_terminal_tab_most_memory_one_terminal_spends()}
		>
			<NumInput
				label={m.settings_terminal_tab_scrollback_memory()}
				unit="MB"
				value={Math.round(t.scrollback_bytes / MB)}
				onchange={(v) => save({ scrollback_bytes: v * MB })}
			/>
		</Row>
	{/if}
	<Row
		id="clipboard"
		label={m.settings_terminal_tab_programs_can_copy_to_your()}
		description={m.settings_terminal_tab_lets_commands_such_as_tmux()}
	>
		<SegmentedControl
			aria-label={m.settings_terminal_tab_programs_can_copy_to_your()}
			size="sm"
			value={t.osc52}
			items={[
				{ value: "ask", label: m.settings_terminal_tab_osc52_ask() },
				{ value: "allow", label: m.settings_terminal_tab_osc52_allow() },
				{ value: "deny", label: m.settings_terminal_tab_osc52_deny() },
			]}
			onchange={(v) => save({ osc52: v as Prefs["terminal"]["osc52"] })}
		/>
	</Row>
	<Row
		id="copy-on-select"
		label={m.settings_terminal_tab_copy_on_select()}
		description={m.settings_terminal_tab_selecting_text_copies_it_to()}
	>
		<Switch
			aria-label={m.settings_terminal_tab_copy_on_select()}
			checked={t.copy_on_select}
			onchange={(v) => save({ copy_on_select: v })}
		/>
	</Row>
	{#if gpu.supported}
		<Row
			id="click-to-move"
			label={m.settings_terminal_tab_click_to_move_the_cursor()}
			description={m.settings_terminal_tab_clicking_on_the_prompt_line()}
		>
			<Switch
				aria-label={m.settings_terminal_tab_click_to_move_the_cursor()}
				checked={t.click_to_move}
				onchange={(v) => save({ click_to_move: v })}
			/>
		</Row>
		<Row
			id="option-as-alt"
			label={m.settings_terminal_tab_option_key_as_alt()}
			description={m.settings_terminal_tab_option_as_alt_description({ key: kbd("alt-b") })}
		>
			<SegmentedControl
				aria-label={m.settings_terminal_tab_option_key_as_alt()}
				size="sm"
				value={t.option_as_alt}
				items={[
					{ value: "both", label: m.settings_terminal_tab_option_both() },
					{ value: "left", label: m.settings_terminal_tab_option_left() },
					{ value: "right", label: m.settings_terminal_tab_option_right() },
					{ value: "none", label: m.settings_capture_timer_off() },
				]}
				onchange={(v) => save({ option_as_alt: v as Prefs["terminal"]["option_as_alt"] })}
			/>
		</Row>
	{/if}
</Group>

<style>
	.select {
		width: 160px;
	}
	.notice {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 12px;
		border-radius: 8px;
		font-size: 12px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 12%, transparent);
	}
	.term {
		margin: 0;
		border-radius: 8px;
		line-height: 1.4;
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		overflow: hidden;
	}
	.sel {
		padding: 0 2px;
	}
	.strip {
		display: flex;
		height: 8px;
		margin-top: 6px;
	}
	.strip span {
		flex: 1;
	}
	.cur {
		display: inline-block;
		width: 0.6em;
		height: 1.1em;
		margin-left: 1px;
		vertical-align: text-bottom;
		background: var(--cur);
	}
	.cur.bar {
		width: 2px;
	}
	.cur.underline {
		height: 2px;
		vertical-align: baseline;
	}
	.cur.hollow {
		background: none;
		box-shadow: inset 0 0 0 1px var(--cur);
	}
	.cur.blink {
		animation: blink 1.1s steps(2, start) infinite;
	}
	@keyframes blink {
		to {
			visibility: hidden;
		}
	}
</style>
