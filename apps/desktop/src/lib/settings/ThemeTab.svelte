<script lang="ts">
	import { Select, SegmentedControl, THEME_PRESETS, colorScheme } from "@codeatlasdev/ui";
	import { Check, Monitor, Moon, Sun } from "@lucide/svelte";
	import { themes } from "#lib/stores/theme.svelte.ts";
	import Group from "./Group.svelte";
	import Row from "./Row.svelte";

	const MODES = [
		{ value: "system", label: "System", icon: Monitor },
		{ value: "light", label: "Light", icon: Sun },
		{ value: "dark", label: "Dark", icon: Moon },
	];

	/** The eight normal ANSI colors of the active theme, as a palette strip (CSS vars as fallback). */
	let palette = $derived(
		Array.from({ length: 8 }, (_, i) => themes.terminal?.ansi16[i + 8] ?? themes.terminal?.ansi16[i] ?? null),
	);
	const NAMES = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
</script>

<Group title="Color theme">
	<Row id="theme-mode" label="Theme" description="Follow your Mac's appearance, or always use light or dark.">
		<SegmentedControl
			aria-label="Theme"
			size="sm"
			value={themes.selection.mode}
			items={MODES}
			onchange={(v) => void themes.setTheme({ mode: v as "system" | "light" | "dark" })}
		/>
	</Row>
	<Row id="theme-light" label="Light theme" description="Colors used in light mode.">
		<div class="select">
			<Select
				size="sm"
				value={themes.selection.light}
				options={themes.names.light.map((n) => ({ value: n, label: n }))}
				onchange={(v) => void themes.setTheme({ light: v })}
			/>
		</div>
	</Row>
	<Row
		id="theme-dark"
		label="Dark theme"
		description="Colors used in dark mode. Add more by dropping Zed theme files into ~/.atlas/desktop/themes."
	>
		<div class="select">
			<Select
				size="sm"
				value={themes.selection.dark}
				options={themes.names.dark.map((n) => ({ value: n, label: n }))}
				onchange={(v) => void themes.setTheme({ dark: v })}
			/>
		</div>
	</Row>
	<Row label="Preview" description={themes.current?.name ?? ""}>
		<div class="palette" aria-hidden="true">
			{#each NAMES as n, i (n)}
				<span style="background: {palette[i] ?? `var(--ansi-${n})`}"></span>
			{/each}
		</div>
	</Row>
</Group>

<Group title="Accent color">
	<Row id="accent" label="Accent color" description="Highlights buttons, selections and focus.">
		<div class="swatches">
			{#each THEME_PRESETS as t (t.id)}
				<button
					class="sw"
					class:on={colorScheme.preset === t.id}
					style="--sw: {t.swatchColor}"
					title={t.label}
					aria-label={t.label}
					aria-pressed={colorScheme.preset === t.id}
					onclick={() => colorScheme.setPreset(t.id)}
				>
					{#if colorScheme.preset === t.id}<Check size={13} />{/if}
				</button>
			{/each}
		</div>
	</Row>
</Group>

<style>
	.select {
		width: 200px;
	}
	.palette {
		display: flex;
		border-radius: 7px;
		overflow: hidden;
		box-shadow: 0 0 0 0.5px var(--ca-outline-subtle);
	}
	.palette span {
		width: 22px;
		height: 22px;
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: 10px;
		max-width: 260px;
	}
	.sw {
		width: 26px;
		height: 26px;
		border-radius: 50%;
		border: 0;
		background: var(--sw);
		color: white;
		display: grid;
		place-items: center;
		box-shadow: inset 0 0 0 0.5px rgb(0 0 0 / 0.2);
	}
	.sw.on {
		box-shadow:
			0 0 0 2px var(--ca-surface-raised),
			0 0 0 4px var(--sw);
	}
</style>
