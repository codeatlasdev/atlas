<script lang="ts">
	/**
	 * A settings page that could not read its values says so. Without it the controls show their defaults, which
	 * look exactly like what the user saved, and a click would overwrite it.
	 */
	import { Alert } from "@codeatlasdev/ui";
	import { m } from "#lib/i18n/m.ts";

	let { what, error }: { what: string; error: string } = $props();
</script>

<Alert severity="error" title={m.settings_general_load_error_could_not_read_what({ what })}>{error}</Alert>
