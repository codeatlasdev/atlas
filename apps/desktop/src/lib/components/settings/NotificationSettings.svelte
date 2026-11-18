<script lang="ts">
	/** Settings ▸ Notifications & sounds ▸ Notifications: when Atlas tells you about agents. */
	import { Switch } from "@codeatlasdev/ui";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
</script>

<Group title={m.settings_general_notification_notifications()} description={m.settings_general_notification_atlas_tells_you_when_an()}>
	<Row
		id="notify"
		label={m.settings_general_notification_show_notifications()}
		description={m.settings_general_notification_a_system_notification_when_an()}
	>
		<Switch
			aria-label={m.settings_general_notification_show_notifications()}
			bind:checked={agents.notify.enabled}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row label={m.settings_general_notification_play_a_sound()} description={m.settings_general_notification_add_a_short_sound_to()}>
		<Switch
			aria-label={m.settings_general_notification_play_a_sound()}
			bind:checked={agents.notify.sound}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
	<Row
		label={m.settings_general_notification_stay_quiet_while_i_m()}
		description={m.settings_general_notification_skip_the_notification_when_that()}
	>
		<Switch
			aria-label={m.settings_general_notification_stay_quiet_while_i_m()}
			bind:checked={agents.notify.suppressWhenFocused}
			onchange={() => agents.saveNotify()}
		/>
	</Row>
</Group>
