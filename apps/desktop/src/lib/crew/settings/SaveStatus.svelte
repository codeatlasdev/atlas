<!-- The "Saving…" / "Saved" / error text next to an autosaved field's label. -->
<script lang="ts">
	import type { AutosaveField } from "#lib/crew/settings/autosaveField.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { field }: { field: AutosaveField } = $props();
</script>

<span
	class="save-status"
	class:saving={field.status === "saving"}
	class:saved={field.status === "saved"}
	class:error={field.status === "error"}
	>{#if field.status === "saving"}{m.common_state_saving()}{:else if field.status === "saved"}{m.crew_settings_save_status_saved()}{:else if field.status === "error"}{field.error}{/if}</span
>

<style>
	.save-status {
		font-size: 11px;
		opacity: 0;
		transition: opacity 200ms;
		pointer-events: none;
	}
	.save-status.saving,
	.save-status.saved,
	.save-status.error {
		opacity: 1;
	}
	.save-status.saving {
		color: var(--ca-text-secondary);
	}
	.save-status.saved {
		color: var(--ca-semantic-success, #22c55e);
	}
	.save-status.error {
		color: var(--ca-semantic-error, #ef4444);
		pointer-events: auto;
	}
</style>
