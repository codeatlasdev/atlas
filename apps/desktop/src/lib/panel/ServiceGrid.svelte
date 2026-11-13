<script lang="ts">
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { tick } from "svelte";
	import { api, type ServiceDef } from "#lib/api.ts";
	import { openService } from "#lib/actions.svelte.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import ServiceCard from "./ServiceCard.svelte";
	import type { ServiceRow } from "./overview.ts";
	import { gridStep, shellIn } from "./panel.ts";
	import { isGreen } from "./serviceState.ts";
	import { flipPlan, orderChanged, type Box } from "./flip.ts";

	type Cmd = "start_service" | "stop_service" | "restart_service";

	/** The service cards: layout, arrow-key navigation and the lifecycle/log/terminal actions. */
	let {
		projectId,
		services,
		running,
		tickers,
		colors,
		onlogs,
	}: {
		projectId: string;
		services: ServiceRow[];
		running: boolean;
		/** Last log line per service name. */
		tickers: Record<string, string>;
		colors: { cpu: string; mem: string };
		/** Reveal the logs tab (the panel also uses it from the timeline). */
		onlogs: (name: string) => void;
	} = $props();

	let grid = $state<HTMLElement | null>(null);
	let cols = $state(1);

	/** Restarting a running service interrupts it: ask first, the way R does from the keyboard. */
	async function act(cmd: Cmd, name: string, viaKey: boolean) {
		if (cmd === "restart_service" && viaKey) {
			const ok = await confirmDialog(`Restart ${name}?`, {
				description: "The service stops and starts again. Connections in flight are dropped.",
				confirmLabel: "Restart",
			});
			if (!ok) return;
		}
		store.run(`svc:${projectId}:${name}`, () => api.service(projectId, cmd, name));
	}

	const space = useSpace();

	function terminal(def: ServiceDef | undefined) {
		const w = space.worktree;
		if (w) void ws.newTerminal(w.id, { argv: shellIn(def?.dir ?? null), label: def?.name ?? "Terminal" });
	}

	async function keynav(e: KeyboardEvent, i: number) {
		const to = gridStep(e.key, i, services.length, cols);
		if (to === i || to < 0) return;
		e.preventDefault();
		store.focusedService = services[to].name;
		await tick();
		grid?.querySelector<HTMLElement>(`[data-service="${CSS.escape(services[to].name)}"]`)?.focus();
	}

	// Cards that move slide to their new place (FLIP). Measured only when the ORDER changed: Svelte's `animate:`
	// reads every cell's rect on every update of a row (a forced layout per cpu/state tick), see flip.ts.
	let order: string[] | null = null;
	let before: Record<string, Box> = {};
	const rects = () =>
		Object.fromEntries(
			[...(grid?.children ?? [])].map((c) => [(c as HTMLElement).dataset.cell!, c.getBoundingClientRect()]),
		);
	$effect.pre(() => {
		const next = services.map((s) => s.name);
		if (orderChanged(order, next)) before = rects();
		const moved = orderChanged(order, next);
		order = next;
		if (!moved) return;
		void tick().then(() => {
			const plan = flipPlan(before, rects());
			for (const c of grid?.children ?? []) {
				const d = plan[(c as HTMLElement).dataset.cell!];
				if (d)
					c.animate([{ translate: `${d.dx}px ${d.dy}px` }, { translate: "0 0" }], {
						duration: 220,
						easing: "ease",
					});
			}
		});
	});

	// The grid only exists once there are services: follow the element, not the mount.
	$effect(() => {
		const el = grid;
		if (!el) return;
		const ro = new ResizeObserver(() => {
			// The computed template lists one track per column: the browser resolved auto-fill for us.
			cols = Math.max(1, getComputedStyle(el).gridTemplateColumns.split(" ").length);
		});
		ro.observe(el);
		return () => ro.disconnect();
	});
</script>

<div
	class="grid"
	bind:this={grid}
	role="group"
	aria-label="Services"
	style:grid-template-columns="repeat(auto-fill, minmax(min(300px, 100%), 1fr))"
	data-tour="service-card"
>
	{#each services as s, i (s.name)}
		<div class="cell" data-cell={s.name}>
			<ServiceCard
				{projectId}
				service={s}
				def={s.def}
				{running}
				busy={!!store.busy[`svc:${projectId}:${s.name}`]}
				focused={store.focusedService === s.name}
				ticker={tickers[s.name] ?? null}
				{colors}
				onfocus={() => (store.focusedService = s.name)}
				onlogs={() => onlogs(s.name)}
				onterminal={() => terminal(s.def)}
				onaction={(cmd) => act(cmd, s.name, cmd === "restart_service" && isGreen(s))}
				onopen={() => s.def && openService(s.def)}
				onkeynav={(e) => keynav(e, i)}
			/>
		</div>
	{/each}
</div>

<style>
	.grid {
		display: grid;
		gap: 12px;
	}
	.cell {
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.cell > :global(.card) {
		flex: 1;
	}
</style>
