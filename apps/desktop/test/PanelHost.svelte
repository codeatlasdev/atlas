<script lang="ts">
	import Panel from "#lib/panel/Panel.svelte";
	import type { ProjectView } from "#lib/api.ts";

	/** The tab group's job for a test: mounts the real `Panel` and lets the test flip `visible` like switching tabs. */
	let { project }: { project: ProjectView } = $props();
	let visible = $state(true);
	export function setVisible(v: boolean) {
		visible = v;
	}
</script>

<Panel {project} {visible} />
