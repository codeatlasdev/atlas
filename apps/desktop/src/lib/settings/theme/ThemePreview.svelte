<script lang="ts">
	/**
	 * Miniature IDE drawn from one theme's data: a gutter, code sample in the theme's syntax roles,
	 * an active line, a selection, an ANSI terminal strip and the accent/surface chips. Every color
	 * comes from the model; nothing here is hard-coded.
	 */
	import type { PreviewModel } from "./preview.ts";
	import { ACTIVE_LINE, SAMPLE, SELECTED_LINE, TERMINAL_LINES } from "./preview.ts";

	let { model, terminalOnly = false }: { model: PreviewModel; terminalOnly?: boolean } = $props();
</script>

<!-- Paints ANOTHER theme (the previewed one) on purpose: the audit skips it; that theme's tokens are tested in theme.test.ts. -->
<div
	class="frame"
	style:background={model.chrome}
	style:--bd={model.border}
	aria-hidden="true"
	data-contrast-ignore
>
	{#if !terminalOnly}
		<div class="editor" style:background={model.editorBg} style:color={model.editorFg}>
			{#each SAMPLE as line, i (i)}
				<div class="line" style:background={i === ACTIVE_LINE ? model.activeLine : undefined}>
					<span class="num" style:color={i === ACTIVE_LINE ? model.gutterActive : model.gutter}>{i + 1}</span>
					<span class="code">
						{#each line as tok, j (j)}
							{@const p = model.syntax[tok.role]}
							<span
								style:color={p.color}
								style:background={i === SELECTED_LINE && j === 1 ? model.selection : undefined}
								style:font-style={p.italic ? "italic" : undefined}
								style:font-weight={p.bold ? 700 : undefined}>{tok.text}</span
							>
						{/each}
					</span>
				</div>
			{/each}
		</div>
	{/if}
	<div class="term" style:background={model.termBg} style:color={model.termFg}>
		{#each TERMINAL_LINES as line, i (i)}
			<div class="tline">
				{#each line as c, j (j)}
					<span style:color={c.ansi === "fg" ? model.termFg : model.ansi[c.ansi]}>{c.text}</span>
				{/each}
				{#if i === TERMINAL_LINES.length - 1}<span class="cur" style:background={model.cursor}></span>{/if}
			</div>
		{/each}
		<div class="ansi">
			{#each model.ansi as c, i (i)}<span style:background={c}></span>{/each}
		</div>
	</div>
	<div class="chips">
		<span class="chip" style:background={model.accent}></span>
		<span class="chip" style:background={model.surface} style:box-shadow="inset 0 0 0 1px {model.border}"
		></span>
		<span class="chip" style:background={model.editorBg} style:box-shadow="inset 0 0 0 1px {model.border}"
		></span>
		<span class="label" style:color={model.muted}
			><!-- i18n-ignore: a type specimen (capital and small a), not a word -->Aa</span
		>
	</div>
</div>

<style>
	.frame {
		display: flex;
		flex-direction: column;
		gap: 0;
		border-radius: 8px;
		overflow: hidden;
		box-shadow: 0 0 0 1px var(--bd);
		font-family: var(--ca-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
		font-size: 8.5px;
		line-height: 1.5;
		user-select: none;
	}
	.editor {
		padding: 5px 0;
	}
	.line {
		display: flex;
		gap: 6px;
		padding: 0 7px 0 0;
		white-space: pre;
	}
	.num {
		width: 18px;
		text-align: right;
		flex-shrink: 0;
	}
	.code {
		overflow: hidden;
	}
	.term {
		padding: 4px 7px 5px;
		white-space: pre;
	}
	.tline {
		display: flex;
		align-items: center;
	}
	.cur {
		display: inline-block;
		width: 5px;
		height: 9px;
		margin-left: 2px;
	}
	.ansi {
		display: grid;
		grid-template-columns: repeat(16, 1fr);
		height: 5px;
		margin-top: 4px;
		border-radius: 2px;
		overflow: hidden;
	}
	.chips {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 7px;
	}
	.chip {
		width: 10px;
		height: 10px;
		border-radius: 50%;
	}
	.label {
		margin-left: auto;
		font-size: 9px;
	}
</style>
