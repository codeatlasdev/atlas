<script lang="ts">
	/**
	 * Grid of theme cards for the chosen appearance filter. Roving tabindex + arrow keys move focus only;
	 * click / Enter / Space selects. Nothing changes the app theme until then.
	 */
	import { SegmentedControl } from "@codeatlasdev/ui";
	import { Moon, Sun, SunMoon } from "@lucide/svelte";
	import { useTheme } from "#lib/shell/themeCommands.svelte.ts";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import { copyTheme, removeTheme, saveTheme } from "./actions.ts";
	import ThemeCard from "./ThemeCard.svelte";
	import { m } from "#lib/i18n/m.ts";

	let filters = $derived([
		{ value: "all", label: m.settings_theme_filter_all(), icon: SunMoon },
		{ value: "dark", label: m.settings_theme_mode_dark(), icon: Moon },
		{ value: "light", label: m.settings_theme_mode_light(), icon: Sun },
	]);

	let filter = $state<"all" | "dark" | "light">("all");
	let shown = $derived(themes.list.filter((t) => filter === "all" || t.appearance === filter));
	let cards = $state<Record<string, HTMLElement | undefined>>({});
	/** Name that holds tabindex 0; null = the selected card (or the first). */
	let roving = $state<string | null>(null);

	let selectedNames = $derived([themes.selection.light, themes.selection.dark]);
	let tabStop = $derived(
		(roving && shown.some((t) => t.name === roving) ? roving : null) ??
			shown.find((t) => selectedNames.includes(t.name))?.name ??
			shown[0]?.name,
	);

	/** Number of columns the auto-fill grid currently lays out, from the real card positions. */
	function columns(): number {
		const tops = shown.map((t) => cards[t.name]?.getBoundingClientRect().top);
		const first = tops[0];
		const n = tops.findIndex((y) => y !== undefined && first !== undefined && Math.abs(y - first) > 4);
		return n === -1 ? shown.length : n;
	}

	function move(e: KeyboardEvent, from: number) {
		if (!(e.target as HTMLElement).dataset.themeCard) return;
		const cols = Math.max(1, columns());
		const step: Record<string, number> = {
			ArrowRight: 1,
			ArrowLeft: -1,
			ArrowDown: cols,
			ArrowUp: -cols,
		};
		let to: number | undefined;
		if (e.key in step) to = from + step[e.key]!;
		else if (e.key === "Home") to = 0;
		else if (e.key === "End") to = shown.length - 1;
		if (to === undefined) return;
		e.preventDefault();
		const next = shown[Math.min(shown.length - 1, Math.max(0, to))];
		if (!next) return;
		roving = next.name;
		cards[next.name]?.focus();
	}
</script>

<div class="wrap">
	<div class="filter">
		<SegmentedControl
			aria-label={m.settings_theme_grid_show_themes()}
			size="sm"
			value={filter}
			items={filters}
			onchange={(v) => (filter = v as typeof filter)}
		/>
		<span class="count">{m.settings_theme_grid_count({ count: shown.length })}</span>
	</div>

	<div class="grid" role="radiogroup" aria-label={m.settings_registry_appearance_group_color_theme()}>
		{#each shown as t, i (t.name)}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div role="presentation" onkeydown={(e) => move(e, i)}>
				<ThemeCard
					theme={t}
					selected={t.name === themes.selection[t.appearance]}
					tabindex={t.name === tabStop ? 0 : -1}
					bind:cardEl={cards[t.name]}
					onpick={() => useTheme(t)}
					oncopy={(f) => copyTheme(t.name, f)}
					onsave={(f) => saveTheme(t.name, f)}
					onremove={() => removeTheme(t.name)}
				/>
			</div>
		{/each}
	</div>
</div>

<style>
	.wrap {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px 14px;
	}
	.filter {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.count {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
		gap: 12px;
	}
</style>
