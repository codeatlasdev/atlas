<script lang="ts">
	/**
	 * A stand-in for the kit's canvas charts (`LineChart`, `BarChart`, `Sparkline`; they need a real browser): the kit
	 * redraws in an effect that reads its data prop, so every time the data it is given is a new value is one draw.
	 * Records `{ kind, data, categories }` per draw in `globalThis.__draws`.
	 */
	let {
		kind = "chart",
		series,
		data,
		categories,
	}: { kind?: string; series?: unknown; data?: unknown; categories?: unknown } = $props();
	$effect(() => {
		(globalThis as { __draws?: unknown[] }).__draws?.push({
			kind,
			data: JSON.stringify(series ?? data),
			categories: JSON.stringify(categories ?? null),
		});
	});
</script>

<span data-stub="chart"></span>
