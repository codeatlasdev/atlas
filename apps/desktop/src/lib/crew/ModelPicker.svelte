<!--
  Model picker for a custom provider (onboarding engine step and the crewmate profile).

  Loads the gateway's models by itself once a base URL and key are filled (no "Fetch models" link), shows them in the
  kit's searchable Combobox, and still accepts a model id that the gateway does not list (`creatable`). Replaced a
  native <datalist>, which dumped every id in an unstyled, unsearchable popup.
-->
<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { Combobox } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import { typeInSearchField } from "#lib/crew/searchKeys.ts";
	import { m } from "#lib/i18n/m.ts";

	let {
		baseUrl,
		apiKey,
		value = $bindable(""),
		id,
	}: { baseUrl: string; apiKey: string; value?: string; id?: string } = $props();

	let models = $state<string[]>([]);
	let loading = $state(false);
	let error = $state("");

	// Re-fetch when the URL or key change, after the user stops typing; a stale answer never overwrites a newer one.
	let seq = 0;
	$effect(() => {
		const base = baseUrl.trim();
		const key = apiKey.trim();
		models = [];
		error = "";
		if (!base || !key) return;
		const mine = ++seq;
		const t = setTimeout(async () => {
			loading = true;
			try {
				const got = await invoke<string[]>("provider_probe", { baseUrl: base, apiKey: key });
				if (mine === seq) models = [...new Set(got)].sort((a, b) => a.localeCompare(b));
			} catch (e) {
				if (mine === seq) error = errText(e);
			} finally {
				if (mine === seq) loading = false;
			}
		}, 500);
		return () => clearTimeout(t);
	});

	const options = $derived(
		[...(value && !models.includes(value) ? [value] : []), ...models].map((name) => ({
			value: name,
			label: name,
		})),
	);
	const placeholder = $derived(
		!baseUrl.trim() || !apiKey.trim()
			? m.crew_workspace_model_picker_fill_first()
			: loading
				? m.crew_workspace_model_picker_loading_models()
				: models.length
					? m.crew_workspace_model_picker_choose({ count: models.length })
					: m.crew_workspace_model_picker_type_id(),
	);
</script>

<!-- Printable keys typed in the search field only type (see searchKeys.ts). -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="model-picker" onkeydowncapture={typeInSearchField}>
	<Combobox
		bind:value={value as string}
		{options}
		{placeholder}
		searchPlaceholder={m.crew_workspace_model_picker_search()}
		emptyText={loading ? m.common_state_loading() : m.crew_workspace_model_picker_no_match()}
		creatable
		ariaLabel={m.crew_settings_engine_section_model()}
		error={error || undefined}
		class={id}
	/>
</div>
