<!-- The connect step's panel: the scene as its hero (most of the panel) and the progress rail under it. -->
<script lang="ts">
	import type { InstallPhase } from "#lib/crew/installSteps.ts";
	import ConnectScene from "./ConnectScene.svelte";
	import InstallChecklist from "./InstallChecklist.svelte";

	let {
		done,
		active,
		failed,
		browsers = false,
	}: {
		done: Set<InstallPhase>;
		active: InstallPhase | null;
		failed: InstallPhase | null;
		browsers?: boolean;
	} = $props();
</script>

<div class="connect-art">
	<ConnectScene {done} {active} {failed} />
	<InstallChecklist {done} {active} {failed} {browsers} />
</div>

<style>
	.connect-art {
		box-sizing: border-box;
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		gap: 14px;
		padding: 28px clamp(20px, 4cqi, 48px) 24px;
	}
</style>
