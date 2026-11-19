<script lang="ts">
	/**
	 * A settings page that could not read its values says so. Without it the controls show their defaults, which
	 * look exactly like what the user saved, and a click would overwrite it.
	 */
	import { Alert } from "@codeatlasdev/ui";
	import { m } from "#lib/i18n/m.ts";

	/** What failed to load: an id, because the sentence is one message per page (a noun phrase cannot be translated alone). */
	type LoadWhat = "agents" | "git" | "workspace" | "phones" | "ssh" | "hygiene";

	let { what, error }: { what: LoadWhat; error: string } = $props();
	let title = $derived.by(() => {
		switch (what) {
			case "agents":
				return m.settings_general_load_error_agents();
			case "git":
				return m.settings_general_load_error_git();
			case "workspace":
				return m.settings_general_load_error_workspace();
			case "phones":
				return m.settings_general_load_error_phones();
			case "ssh":
				return m.settings_general_load_error_ssh();
			case "hygiene":
				return m.settings_general_load_error_hygiene();
		}
	});
</script>

<Alert severity="error" {title}>{error}</Alert>
