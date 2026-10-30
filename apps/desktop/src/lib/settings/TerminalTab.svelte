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

	let p = $derived(prefs.value);
	let t = $derived(p.terminal);
	const MB = 1024 * 1024;
	const save = (patch: Partial<Prefs["terminal"]>) => savePref("terminal", { ...t, ...patch });

	/** Ends every terminal: the app reconnects and starts the current host. */
	async function restart() {
		const ok = await confirmDialog("Restart the terminal host?", {
			description: "Running terminals and agents end. Tabs stay; use Restart on each to reopen them.",
			confirmLabel: "Restart",
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

<Group title="Renderer">
	{#if gpu.supported}
		<Row
			id="gpu"
			label="Fast terminal (GPU)"
			description="Draws local terminals on the graphics chip for smoother scrolling and faster output. Applies to terminals you open afterwards. SSH terminals always use the classic renderer."
		>
			<Switch aria-label="Fast terminal (GPU)" checked={gpu.terminal} onchange={(v) => setGpuTerminal(v)} />
			{#snippet below()}
				{#if gpu.terminal && gpu.hostTooOld}
					<div class="notice">
						<span
							>The running terminal host is from an older Atlas, so terminals use the classic renderer.</span
						>
						<Button size="sm" variant="outlined" onclick={restart}>Restart terminal host</Button>
					</div>
				{/if}
			{/snippet}
		</Row>
	{/if}
	<Row
		id="host-restart"
		label="Terminal host"
		description="The background process that keeps terminals alive. Restart it to pick up a new Atlas version; running terminals end."
	>
		<Button size="sm" variant="outlined" onclick={restart}>Restart terminal host</Button>
	</Row>
</Group>

<Group title="Text">
	<Row
		id="term-font"
		label="Terminal font"
		description="Font families in order, like a CSS list. The first one installed wins; the fast terminal falls back to JetBrains Mono. Variable fonts work."
		stack
	>
		<TextInput
			mono
			width="100%"
			label="Terminal font"
			value={t.font_family}
			onchange={(v) => save({ font_family: v })}
		/>
	</Row>
	<Row id="term-size" label="Terminal font size" description="Text size in terminals.">
		<NumInput
			label="Terminal font size"
			unit="px"
			value={t.font_size}
			onchange={(v) => save({ font_size: v })}
		/>
	</Row>
	<Row
		id="font-thicken"
		label="Thicken glyphs"
		description="Draw text slightly heavier. Helps thin fonts on dark backgrounds."
	>
		<Switch
			aria-label="Thicken glyphs"
			checked={t.font_thicken}
			onchange={(v) => save({ font_thicken: v })}
		/>
	</Row>
	<Row
		id="ligatures"
		label="Ligatures"
		description="Join characters like -> and != into single symbols. Fast terminal only."
	>
		<Switch aria-label="Ligatures" checked={t.ligatures} onchange={(v) => save({ ligatures: v })} />
	</Row>
	<Row
		id="contrast"
		label="Minimum contrast"
		description="Lightens or darkens text that is hard to read on its background. 4.5 (the default) meets WCAG AA; 1 turns it off. Fast terminal only."
	>
		<NumInput
			label="Minimum contrast"
			value={t.minimum_contrast}
			onchange={(v) => save({ minimum_contrast: v, minimum_contrast_chosen: true })}
		/>
	</Row>
</Group>

<Group title="Cursor">
	<Row id="cursor-shape" label="Cursor shape" description="How the cursor looks in terminals.">
		<div class="select">
			<Select
				size="sm"
				value={t.cursor_shape}
				options={[
					{ value: "block", label: "Block" },
					{ value: "bar", label: "Bar" },
					{ value: "underline", label: "Underline" },
					{ value: "hollow", label: "Hollow" },
				]}
				onchange={(v) => save({ cursor_shape: v as Prefs["terminal"]["cursor_shape"] })}
			/>
		</div>
	</Row>
	<Row id="cursor-blink" label="Blinking cursor" description="Make the cursor blink.">
		<Switch aria-label="Blinking cursor" checked={t.blinking} onchange={(v) => save({ blinking: v })} />
	</Row>
	<Row label="Preview">
		{#snippet below()}
			<div
				class="term"
				style="background: {bg}; color: {fg}; font-family: {t.font_family}; font-size: {t.font_size}px; font-weight: {t.font_thicken
					? 500
					: 400}; padding: {t.padding_y}px {t.padding_x}px"
			>
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

<Group title="Padding">
	<Row
		id="term-padding-x"
		label="Horizontal padding"
		description="Space left and right of the text. The margin uses the terminal background."
	>
		<NumInput
			label="Horizontal padding"
			unit="pt"
			value={t.padding_x}
			onchange={(v) => save({ padding_x: v })}
		/>
	</Row>
	<Row
		id="term-padding-y"
		label="Vertical padding"
		description="Space above and below the text. The margin uses the terminal background."
	>
		<NumInput
			label="Vertical padding"
			unit="pt"
			value={t.padding_y}
			onchange={(v) => save({ padding_y: v })}
		/>
	</Row>
</Group>

<Group title="Behavior">
	<Row
		id="scrollback"
		label="Scrollback"
		description="Lines of output each terminal remembers. More lines use more memory."
	>
		<NumInput
			label="Scrollback"
			unit="lines"
			value={t.max_scroll_history_lines}
			onchange={(v) => save({ max_scroll_history_lines: v })}
		/>
	</Row>
	<Row
		id="scrollback-memory"
		label="Scrollback memory"
		description="Most memory one terminal spends on its scrollback. A wide terminal keeps fewer lines than the setting above to stay inside it (fast terminal only)."
	>
		<NumInput
			label="Scrollback memory"
			unit="MB"
			value={Math.round(t.scrollback_bytes / MB)}
			onchange={(v) => save({ scrollback_bytes: v * MB })}
		/>
	</Row>
	<Row
		id="clipboard"
		label="Programs can copy to your clipboard"
		description="Lets commands, such as tmux or a remote shell, put text on your clipboard."
	>
		<SegmentedControl
			aria-label="Programs can copy to your clipboard"
			size="sm"
			value={t.osc52}
			items={[
				{ value: "ask", label: "Ask each time" },
				{ value: "allow", label: "Always allow" },
				{ value: "deny", label: "Never" },
			]}
			onchange={(v) => save({ osc52: v as Prefs["terminal"]["osc52"] })}
		/>
	</Row>
	<Row
		id="copy-on-select"
		label="Copy on select"
		description="Selecting text copies it to the clipboard as soon as you release the mouse. Fast terminal only."
	>
		<Switch
			aria-label="Copy on select"
			checked={t.copy_on_select}
			onchange={(v) => save({ copy_on_select: v })}
		/>
	</Row>
	<Row
		id="click-to-move"
		label="Click to move the cursor"
		description="Clicking on the prompt line moves the cursor there. Needs shell integration (OSC 133 prompt marks); fast terminal only."
	>
		<Switch
			aria-label="Click to move the cursor"
			checked={t.click_to_move}
			onchange={(v) => save({ click_to_move: v })}
		/>
	</Row>
	<Row
		id="option-as-alt"
		label="Option key as Alt"
		description="Makes ⌥ send Alt (⌥B jumps back a word) instead of typing a macOS character such as ∫. Fast terminal only."
	>
		<SegmentedControl
			aria-label="Option key as Alt"
			size="sm"
			value={t.option_as_alt}
			items={[
				{ value: "both", label: "Both" },
				{ value: "left", label: "Left" },
				{ value: "right", label: "Right" },
				{ value: "none", label: "Off" },
			]}
			onchange={(v) => save({ option_as_alt: v as Prefs["terminal"]["option_as_alt"] })}
		/>
	</Row>
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
