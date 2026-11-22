<script lang="ts">
	import { Cpu, Layers, MemoryStick, RotateCw, TriangleAlert } from "@lucide/svelte";
	import { fmtMem } from "#lib/format.ts";
	import Kpi from "./Kpi.svelte";
	import { m } from "#lib/i18n/m.ts";

	/** The five headline tiles: service health, restarts, errors, memory and CPU. */
	let {
		running,
		up,
		total,
		failed,
		restarts,
		errors,
		mem,
		cpu,
	}: {
		running: boolean;
		up: number;
		total: number;
		failed: number;
		restarts: number;
		errors: number;
		mem: number;
		cpu: number;
	} = $props();
</script>

<div class="kpis" data-tour="panel-kpis">
	<Kpi
		icon={Layers}
		label={m.panel_kpis_services()}
		value="{up}/{total}"
		hint={running ? m.panel_state_running() : m.panel_state_stopped()}
		tone={failed ? "fail" : running && up === total ? "ok" : "off"}
	/>
	<Kpi
		icon={RotateCw}
		label={m.panel_kpis_restarts()}
		value={String(restarts)}
		hint={m.panel_kpis_hint_last_hour()}
		tone={restarts ? "warn" : "off"}
	/>
	<Kpi
		icon={TriangleAlert}
		label={m.panel_kpis_errors()}
		value={String(errors)}
		hint={m.panel_kpis_hint_last_5_min()}
		tone={errors ? "fail" : "off"}
	/>
	<Kpi
		icon={MemoryStick}
		label={m.panel_kpis_memory()}
		value={running ? fmtMem(Math.round(mem)) : "—"}
		hint={m.panel_kpis_hint_all_services()}
		tone="off"
	/>
	<Kpi
		icon={Cpu}
		label={m.panel_kpis_cpu()}
		value={running ? `${Math.round(cpu)}%` : "—"}
		hint={m.panel_kpis_hint_of_one_core()}
		tone="off"
	/>
</div>

<style>
	.kpis {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
		gap: 10px;
	}
</style>
