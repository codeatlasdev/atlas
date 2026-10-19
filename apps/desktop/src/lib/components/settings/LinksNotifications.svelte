<script lang="ts">
	import { IS_MAC } from "#lib/keys.ts";
	/** Settings ▸ General ▸ Links and Notifications: where links open and when Atlas tells you about agents. */
	import { SegmentedControl, Switch } from "@codeatlasdev/ui";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
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

<Group title="Notifications" description="Atlas tells you when an agent finishes or needs you.">
	<Row
		id="notify"
		label="Show notifications"
		description="A system notification when an agent is done or is waiting for an answer."
	>
		<Switch
			aria-label="Show notifications"
			bind:checked={agents.notify.enabled}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row label="Play a sound" description="Add a short sound to each notification.">
		<Switch
			aria-label="Play a sound"
			bind:checked={agents.notify.sound}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row
		label="Stay quiet while I'm watching"
		description="Skip the notification when that worktree is the one on your screen."
	>
		<Switch
			aria-label="Stay quiet while I'm watching"
			bind:checked={agents.notify.suppressWhenFocused}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
</Group>
