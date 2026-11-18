<script lang="ts">
	/** The light editor's Markdown formatting bar: bold, italic, heading, lists and link over the open document. */
	import type { EditorView } from "@codemirror/view";
	import { Bold, Eye, Heading, Italic, Link2, List, ListOrdered, Pencil } from "@lucide/svelte";
	import type { IconComponent } from "@codeatlasdev/ui";
	import {
		cycleHeading,
		insertLink,
		toggleBold,
		toggleBullets,
		toggleItalic,
		toggleNumbered,
	} from "#lib/editor/markdownFormat.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		view,
		preview,
		onpreview,
	}: {
		/** The editor's view; `undefined` while the file loads (the buttons are disabled then). */
		view: EditorView | undefined;
		/** The rendered preview is showing instead of the source. */
		preview: boolean;
		onpreview: (on: boolean) => void;
	} = $props();

	interface Item {
		label: () => string;
		icon: IconComponent;
		run: (v: EditorView) => boolean;
	}
	const ITEMS: Item[] = [
		{ label: () => m.workbench_markdown_bar_bold(), icon: Bold, run: toggleBold },
		{ label: () => m.workbench_markdown_bar_italic(), icon: Italic, run: toggleItalic },
		{ label: () => m.workbench_markdown_bar_heading(), icon: Heading, run: cycleHeading },
		{ label: () => m.workbench_markdown_bar_bulleted_list(), icon: List, run: toggleBullets },
		{ label: () => m.workbench_markdown_bar_numbered_list(), icon: ListOrdered, run: toggleNumbered },
		{ label: () => m.workbench_markdown_bar_link(), icon: Link2, run: insertLink },
	];

	function press(item: Item) {
		if (!view) return;
		item.run(view);
		view.focus();
	}
</script>

<div class="mdbar" role="toolbar" aria-label={m.workbench_markdown_bar_markdown_formatting()}>
	{#each ITEMS as item (item.icon)}
		<button
			type="button"
			aria-label={item.label()}
			title={item.label()}
			disabled={!view || preview}
			onclick={() => press(item)}
		>
			<item.icon size={14} />
		</button>
	{/each}
	<span class="grow"></span>
	<button
		type="button"
		class:on={preview}
		aria-pressed={preview}
		aria-label={preview ? m.workbench_markdown_bar_edit_source() : m.workbench_markdown_bar_preview()}
		title={preview ? m.workbench_markdown_bar_edit_source() : m.workbench_markdown_bar_preview()}
		disabled={!view}
		onclick={() => onpreview(!preview)}
	>
		{#if preview}<Pencil size={14} />{:else}<Eye size={14} />{/if}
	</button>
</div>

<style>
	.mdbar {
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 3px 10px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.grow {
		margin-right: auto;
	}
	button {
		display: inline-flex;
		padding: 5px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
	}
	button:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	button:disabled {
		opacity: 0.4;
	}
	button.on {
		color: var(--ca-brand-main);
		background: var(--ca-action-hover);
	}
</style>
