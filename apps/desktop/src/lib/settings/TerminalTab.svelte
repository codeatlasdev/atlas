<script lang="ts">
	import { Button, Select, SegmentedControl, Switch } from "@codeatlasdev/ui";
	import { gpu, setGpuTerminal } from "#lib/stores/gpu.svelte.ts";
	import { prefs } from "#lib/stores/prefs.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import type { Prefs } from "#lib/prefs.ts";
	import { hostCall } from "#lib/host.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import Group from "./Group.svelte";
	import NumInput from "./NumInput.svelte";
	import Row from "./Row.svelte";
	import TextInput from "./TextInput.svelte";
	import { savePref } from "./savePref.ts";

	let p = $derived(prefs.value);
	let t = $derived(p.terminal);
	const save = (patch: Partial<Prefs["terminal"]>) => savePref("terminal", { ...t, ...patch });

	/** Ends every terminal: the app reconnects and starts the current host. */
	async function restartHost() {
		const ok = await confirmDialog("Restart the terminal host?", {
			description: "Running terminals and agents end. Tabs stay; use Restart on each to reopen them.",
			confirmLabel: "Restart",
			destructive: true,
		});
		if (!ok) return;
		await hostCall("shutdown").catch(() => {});
		gpu.hostTooOld = false;
	}

	let colors = $derived(themes.terminal);
	let bg = $derived(colors?.bg ?? "var(--ca-surface-sunken)");
	let fg = $derived(colors?.fg ?? "var(--ca-text-primary)");
	const ansi = (i: number, fallback: string) => colors?.ansi16[i] ?? `var(--ansi-${fallback})`;
</script>

<Group title="Renderer">
	<Row
		id="gpu"
		label="Fast terminal (GPU)"
		description="Draws local terminals on the graphics chip for smoother scrolling and faster output. Applies to terminals you open afterwards. SSH terminals always use the classic renderer."
	>
		<Switch aria-label="Fast terminal (GPU)" checked={gpu.terminal} onchange={(v) => setGpuTerminal(v)} />
		{#snippet below()}
			{#if gpu.terminal && gpu.hostTooOld}
				<div class="notice">
					<span>The running terminal host is from an older Atlas, so terminals use the classic renderer.</span
					>
					<Button size="sm" variant="outlined" onclick={restartHost}>Restart terminal host</Button>
				</div>
			{/if}
		{/snippet}
	</Row>
</Group>

<Group title="Text">
	<Row
		id="term-font"
		label="Terminal font"
		description="Font used by the classic terminal. The fast terminal always uses JetBrains Mono."
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
		id="ligatures"
		label="Ligatures"
		description="Join characters like -> and != into single symbols. Fast terminal only."
	>
		<Switch aria-label="Ligatures" checked={t.ligatures} onchange={(v) => save({ ligatures: v })} />
	</Row>
	<Row
		id="contrast"
		label="Minimum contrast"
		description="Lightens or darkens text that is hard to read on its background. 1 turns it off; 4.5 meets accessibility guidance. Fast terminal only."
	>
		<NumInput
			label="Minimum contrast"
			value={t.minimum_contrast}
			onchange={(v) => save({ minimum_contrast: v })}
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
			<pre class="term" style="background: {bg}; color: {fg}; font-size: {t.font_size}px"><span
					style="color: {ansi(2, 'green')}">~/atlas</span
				> <span style="color: {ansi(4, 'blue')}">main</span> $ bun run dev<span
					class="cur {t.cursor_shape}"
					class:blink={t.blinking}
					style="--cur: {fg}"></span></pre>
		{/snippet}
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
		padding: 10px 12px;
		border-radius: 8px;
		font-family: ui-monospace, "SF Mono", Menlo, monospace;
		line-height: 1.4;
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
		overflow: hidden;
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
