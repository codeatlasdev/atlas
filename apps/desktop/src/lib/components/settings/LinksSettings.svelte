<script lang="ts">
	/** Settings ▸ Integrations ▸ Shell & links ▸ Links: which browser opens the links Atlas shows. */
	import { SegmentedControl } from "@codeatlasdev/ui";
	import { IS_MAC } from "#lib/keys.ts";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { browserPrefs } from "#lib/stores/browser.svelte.ts";
</script>

<Group title="Links">
	<Row
		id="browser"
		label="Open links in"
		description={`Where links from services, ports and terminals open. Hold ${IS_MAC ? "⌘" : "Ctrl"} while clicking to use the other browser.`}
	>
		<SegmentedControl
			aria-label="Open links in"
			size="sm"
			value={browserPrefs.openIn}
			items={[
				{ value: "atlas", label: "Atlas browser" },
				{ value: "system", label: "System browser" },
			]}
			onchange={(v) => browserPrefs.set(v as "atlas" | "system")}
		/>
	</Row>
</Group>
