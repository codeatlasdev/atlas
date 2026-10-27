<script lang="ts">
	/** Slide "one atlas.yaml": the file on the left, its services coming up one by one on the right. */
	import { onMount } from "svelte";
	import Frame from "./Frame.svelte";
	import { Beats } from "./beats.svelte.ts";

	const SERVICES = [
		{ name: "db", port: 5432 },
		{ name: "api", port: 8080 },
		{ name: "web", port: 3000 },
		{ name: "worker", port: null },
	];
	// 0 file, 1–4 services start (each: starting at k, running at k+1), 5 all up, hold.
	const beats = new Beats([700, 450, 450, 450, 450, 2600]);
	onMount(() => beats.start());
	const state = (i: number) => (beats.n > i + 1 ? "running" : beats.n > i ? "starting" : "idle");
</script>

<Frame title="shop · atlas.yaml">
	<div class="split">
		<pre class="yaml" aria-hidden="true"><span class="k">services:</span>
{#each SERVICES as s, i (s.name)}<span class="line" class:on={beats.n > i}>  <span class="k">{s.name}:</span>
    <span class="v">command: …</span>{#if s.port}
						<span class="v">port: {s.port}</span>{/if}
</span>{/each}</pre>
		<ul class="svc" aria-hidden="true">
			{#each SERVICES as s, i (s.name)}
				<li class={state(i)}>
					<span class="led"></span>
					<b>{s.name}</b>
					<span class="port">{s.port ? `:${s.port}` : "job"}</span>
				</li>
			{/each}
			<li class="all" class:on={beats.n === beats.last}>All up · 4 services</li>
		</ul>
	</div>
</Frame>

<style>
	.split {
		position: absolute;
		inset: 0;
		display: grid;
		grid-template-columns: 1fr 1fr;
	}
	.yaml {
		margin: 0;
		padding: 14px 16px;
		font: 11.5px/1.55 var(--mono);
		color: var(--ca-text-secondary);
		border-right: 0.5px solid var(--ca-outline-subtle);
		overflow: hidden;
	}
	.k {
		color: var(--ca-brand-main);
	}
	.line {
		opacity: 0.35;
		transition: opacity 300ms ease;
	}
	.line.on {
		opacity: 1;
	}
	.svc {
		list-style: none;
		margin: 0;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	li {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 10px;
		border-radius: 9px;
		font-size: 12px;
		background: var(--ca-surface-sunken);
		opacity: 0.45;
		transform: translateX(6px);
		transition:
			opacity 320ms cubic-bezier(0.2, 0.8, 0.2, 1),
			transform 320ms cubic-bezier(0.2, 0.8, 0.2, 1);
	}
	li.starting,
	li.running {
		opacity: 1;
		transform: none;
	}
	.port {
		margin-left: auto;
		font: 11px var(--mono);
		color: var(--ca-text-secondary);
	}
	.led {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-text-disabled);
		transition: background-color 260ms;
	}
	.starting .led {
		background: var(--ca-semantic-warning);
	}
	.running .led {
		background: var(--ca-semantic-success);
	}
	.all {
		justify-content: center;
		margin-top: 4px;
		background: color-mix(in oklab, var(--ca-semantic-success) 14%, transparent);
		color: var(--ca-semantic-success);
		font-weight: 600;
		opacity: 0;
	}
	.all.on {
		opacity: 1;
		transform: none;
	}
</style>
