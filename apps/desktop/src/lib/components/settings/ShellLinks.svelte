<script lang="ts">
	/** Settings ▸ Integrations ▸ Shell & links: the shell projects start with, and where links open. */
	import ShellSettings from "../ShellSettings.svelte";
	import LinksSettings from "./LinksSettings.svelte";
	import SectionTitle from "./SectionTitle.svelte";
</script>

<SectionTitle title="Shell for your projects" setting="shell" first>
	Services start with the PATH and variables of this shell, the same as in your terminal. Pick another shell
	if tools like bun or node are not found.
</SectionTitle>
<ShellSettings />
<LinksSettings />
