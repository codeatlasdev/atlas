<script lang="ts">
	/** Settings ▸ Integrations ▸ Shell & links: the shell projects start with, and where links open. */
	import ShellSettings from "../ShellSettings.svelte";
	import LinksSettings from "./LinksSettings.svelte";
	import SectionTitle from "./SectionTitle.svelte";
	import { m } from "#lib/i18n/m.ts";
</script>

<SectionTitle title={m.settings_general_shell_links_shell_for_your_projects()} setting="shell" first>
	{m.settings_general_shell_links_services_start_with_the_path()}
</SectionTitle>
<ShellSettings />
<LinksSettings />
