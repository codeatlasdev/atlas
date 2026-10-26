<script lang="ts">
	/**
	 * A settings page that could not read its values says so. Without it the controls show their defaults, which
	 * look exactly like what the user saved, and a click would overwrite it.
	 */
	import { Alert } from "@codeatlasdev/ui";

	let { what, error }: { what: string; error: string } = $props();
</script>

<Alert severity="error" title="Could not read {what}">{error}</Alert>
