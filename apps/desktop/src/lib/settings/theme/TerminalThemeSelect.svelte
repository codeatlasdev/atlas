<script lang="ts">
	/** `Terminal theme` control: "Same as app" or any theme, with a mini terminal showing the choice. */
	import { Select } from "@codeatlasdev/ui";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import ThemePreview from "./ThemePreview.svelte";
	import { previewModel } from "./preview.ts";
	import { m } from "#lib/i18n/m.ts";

	const options = $derived([
		{ value: "app", label: m.settings_theme_terminal_same_as_app() },
		...themes.list.map((t) => ({ value: t.name, label: t.name })),
	]);
	// The terminal follows the app theme unless a theme of its own is chosen.
	const shown = $derived(
		themes.selection.terminal === "app"
			? themes.current
			: (themes.list.find((t) => t.name === themes.selection.terminal) ?? themes.current),
	);
	let model = $derived(shown && previewModel(shown));
</script>

<div class="wrap">
	<div class="select">
		<Select
			size="sm"
			value={themes.selection.terminal}
			{options}
			onchange={(v) => void themes.setTheme({ terminal: v })}
		/>
	</div>
	{#if model}
		<div class="mini"><ThemePreview {model} terminalOnly /></div>
	{/if}
</div>

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 8px;
	}
	.select {
		width: 200px;
	}
	.mini {
		width: 200px;
	}
</style>
