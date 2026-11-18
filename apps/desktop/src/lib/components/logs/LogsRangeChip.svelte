<script lang="ts">
	import { Clock, X } from "@lucide/svelte";
	import { ALL_TIME, LAST_PRESETS, parseCustomRange, rangeLabel, type TimeRange } from "#lib/logs/range.ts";
	import { m } from "#lib/i18n/m.ts";

	let { range = $bindable(), only }: { range: TimeRange; only: string | null } = $props();

	let open = $state(false);
	let customText = $state("");
	let customError = $state("");
	let label = $derived(rangeLabel(range));

	function pick(r: TimeRange) {
		range = r;
		customError = "";
		open = false;
	}
	function applyCustom() {
		const res = parseCustomRange(customText);
		if ("error" in res) customError = res.error;
		else {
			customText = "";
			pick(res.range);
		}
	}
	function onkey(ev: KeyboardEvent) {
		ev.stopPropagation();
		if (ev.key === "Enter") applyCustom();
		else if (ev.key === "Escape") open = false;
		else customError = "";
	}
</script>

<svelte:window
	onpointerdown={(ev) => {
		if (open && !(ev.target as Element).closest?.(".rangewrap")) open = false;
	}}
/>

<div class="rangewrap">
	<div class="rangechip" class:on={range.kind !== "all"}>
		<button
			class="rangebtn"
			title={m.logs_range_chip_time_range()}
			aria-label={m.logs_range_chip_time_range_label({ label })}
			aria-haspopup="dialog"
			aria-expanded={open}
			onclick={() => (open = !open)}
		>
			<Clock size={13} />
			{#if range.kind !== "all"}<span>{label}</span>{/if}
		</button>
		{#if range.kind !== "all"}
			<button
				class="rangex"
				title={m.logs_range_chip_clear_time_range()}
				aria-label={m.logs_range_chip_clear_time_range()}
				onclick={() => pick(ALL_TIME)}><X size={11} /></button
			>
		{/if}
	</div>
	{#if open}
		<div class="rangepop" role="dialog" aria-label={m.logs_range_chip_time_range()}>
			{#each LAST_PRESETS as minutes (minutes)}
				<button
					class="opt"
					class:sel={range.kind === "last" && range.minutes === minutes}
					onclick={() => pick({ kind: "last", minutes })}>{rangeLabel({ kind: "last", minutes })}</button
				>
			{/each}
			<button class="opt" class:sel={range.kind === "restart"} onclick={() => pick({ kind: "restart" })}
				>{only
					? m.logs_range_chip_since_service_restarted({ name: only })
					: m.logs_range_chip_since_last_restart()}</button
			>
			<button class="opt" class:sel={range.kind === "start"} onclick={() => pick({ kind: "start" })}
				>{rangeLabel({ kind: "start" })}</button
			>
			<div class="custom">
				<input
					bind:value={customText}
					placeholder={m.logs_range_chip_custom_placeholder()}
					aria-label={m.logs_range_chip_custom_time_range_today()}
					aria-invalid={!!customError}
					spellcheck="false"
					autocomplete="off"
					onkeydown={onkey}
				/>
				<button class="toggle" onclick={applyCustom}>{m.logs_range_chip_apply()}</button>
			</div>
			{#if customError}<div class="rangeerr" role="alert">{customError}</div>{/if}
		</div>
	{/if}
</div>

<style>
	.rangewrap {
		position: relative;
	}
	.rangechip {
		display: inline-flex;
		align-items: center;
		height: 26px;
		border-radius: 7px;
		border: 0.5px solid transparent;
		color: var(--ca-text-secondary);
	}
	.rangechip.on {
		border-color: var(--ca-outline-subtle);
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.rangebtn,
	.rangex {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 100%;
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		font-size: 11px;
		cursor: pointer;
	}
	.rangebtn {
		padding: 0 8px;
		border-radius: 7px;
	}
	.rangex {
		padding: 0 6px 0 0;
	}
	.rangechip:not(.on) .rangebtn:hover {
		background: var(--ca-action-hover);
	}
	.rangepop {
		position: absolute;
		z-index: 20;
		top: calc(100% + 4px);
		left: 0;
		min-width: 200px;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: 4px;
		border-radius: 9px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-overlay);
		box-shadow: 0 6px 24px color-mix(in oklab, black 30%, transparent);
	}
	.opt {
		text-align: left;
		font: inherit;
		font-size: 12px;
		padding: 5px 8px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-primary, inherit);
		cursor: pointer;
	}
	.opt:hover {
		background: var(--ca-action-hover);
	}
	.opt.sel {
		color: var(--ca-brand-fill);
		font-weight: 600;
	}
	.custom {
		display: flex;
		gap: 6px;
		padding: 6px 4px 2px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		margin-top: 3px;
	}
	.custom input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: inherit;
	}
	.custom input[aria-invalid="true"] {
		border-color: var(--ca-semantic-error);
	}
	.rangeerr {
		padding: 2px 8px 4px;
		font-size: 11px;
		color: var(--ca-semantic-error);
	}
	.toggle {
		font: inherit;
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
</style>
