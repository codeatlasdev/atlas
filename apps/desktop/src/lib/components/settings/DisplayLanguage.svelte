<script lang="ts">
	/**
	 * Settings ▸ General ▸ Display language: the language of the app's own text. Not the Languages tab (that one is
	 * programming languages). Saving goes through Rust (`~/.atlas/language`, shared with the CLI) and the UI switches
	 * live, no reload.
	 */
	import { Select, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import {
		LOCALE_NAMES,
		appLocale,
		chooseLocale,
		localePreference,
		localeSource,
	} from "#lib/i18n/locale.svelte.ts";
	import { m } from "#lib/i18n/m.ts";
	import { locales, type Locale } from "#lib/paraglide/runtime.js";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";

	const FOLLOW = "system";

	// Computed here, not at module level: the first option's text follows the language. While the app follows the system
	// the language that results is named after it (in its own name); with a saved choice the system's language is not
	// known to the page, so the bare words are used.
	let options = $derived([
		{
			value: FOLLOW,
			label:
				localePreference() === null
					? m.settings_display_language_match_system_with({ language: LOCALE_NAMES[appLocale()] })
					: m.settings_display_language_match_system(),
		},
		...locales.map((l) => ({ value: l, label: LOCALE_NAMES[l] })),
	]);

	async function pick(value: string) {
		try {
			await chooseLocale(value === FOLLOW ? null : (value as Locale));
		} catch (e) {
			toast.error(m.settings_display_language_failed({ error: errText(e) }));
		}
	}
</script>

<Group title={m.settings_group_language()}>
	<Row
		id="display-language"
		label={m.settings_display_language_label()}
		description={localeSource() === "env"
			? m.settings_display_language_env_override()
			: m.settings_display_language_description()}
	>
		<div class="select">
			<Select
				size="sm"
				aria-label={m.settings_display_language_label()}
				value={localePreference() ?? FOLLOW}
				{options}
				onchange={pick}
			/>
		</div>
	</Row>
</Group>

<style>
	.select {
		width: 200px;
	}
</style>
