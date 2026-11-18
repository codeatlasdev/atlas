<script lang="ts">
	/** Settings ▸ Integrations ▸ Shell & links ▸ Links: which browser opens the links Atlas shows. */
	import { SegmentedControl } from "@codeatlasdev/ui";
	import { modClick } from "#lib/keys.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { browserPrefs } from "#lib/stores/browser.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
</script>

<Group title={m.settings_general_links_links()}>
	<Row
		id="browser"
		label={m.settings_general_links_open_links_in()}
		description={m.settings_general_links_description({ key: modClick() })}
	>
		<SegmentedControl
			aria-label={m.settings_general_links_open_links_in()}
			size="sm"
			value={browserPrefs.openIn}
			items={[
				{ value: "atlas", label: m.settings_general_links_atlas_browser() },
				{ value: "system", label: m.settings_general_links_system_browser() },
			]}
			onchange={(v) => browserPrefs.set(v as "atlas" | "system")}
		/>
	</Row>
</Group>
