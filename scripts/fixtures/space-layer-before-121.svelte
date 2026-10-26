<!-- The pager layer as #115 shipped it (root carries `inert`, `pointer-events: none`, `contain: ... style`): the design #121 replaced. Kept only so scripts/pager-restyle-probe.test.ts can prove the probe rejects it. Not part of the app. -->
<script lang="ts">
	let { id, current, side, vars, layer, children } = $props();
</script>

<div
	class="space-layer"
	style:--space-accent={vars.accent}
	data-space={id}
	data-role={current ? "current" : "near"}
	data-side={current ? undefined : side}
	inert={!current}
	aria-hidden={current ? undefined : true}
	use:layer={id}
>
	<div class="space-body">{@render children()}</div>
</div>

<style>
	.space-layer { position: absolute; inset: 0; display: flex; flex-direction: column; min-height: 0; }
	.space-body { flex: 1; display: flex; flex-direction: column; min-height: 0; }
	.space-layer[data-role="near"] { contain: layout paint style; pointer-events: none; will-change: transform; transform: translate3d(100%, 0, 0); }
	.space-layer[data-side="prev"] { transform: translate3d(-100%, 0, 0); }
</style>
