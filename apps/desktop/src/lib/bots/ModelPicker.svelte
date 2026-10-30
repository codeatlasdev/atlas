<!--
  Model picker for a custom provider (onboarding engine step and the bot profile).

  Loads the gateway's models by itself once a base URL and key are filled (no "Fetch models" link), shows them in the
  kit's searchable Combobox, and still accepts a model id that the gateway does not list (`creatable`). Replaced a
  native <datalist>, which dumped every id in an unstyled, unsearchable popup.
-->
<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { Combobox } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";

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
		[...(value && !models.includes(value) ? [value] : []), ...models].map((m) => ({ value: m, label: m })),
	);
	const placeholder = $derived(
		!baseUrl.trim() || !apiKey.trim()
			? "Fill the URL and key first"
			: loading
				? "Loading models…"
				: models.length
					? `Choose one of ${models.length} models`
					: "Type a model id",
	);
</script>

<!-- The kit's Combobox forwards every key of its search field to the listbox, whose type-to-select swallows a letter
     that starts an item ("a" for "anthropic/…", "auto/…") and jumps the highlight instead of typing it. Printable keys
     typed in the search field stop here, so they only type; arrows, Enter and Esc still reach the list. -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="model-picker"
	onkeydowncapture={(e) => {
		const t = e.target as HTMLElement;
		if (
			t.classList.contains("ca-combobox__input") &&
			e.key.length === 1 &&
			!e.metaKey &&
			!e.ctrlKey &&
			!e.altKey
		)
			e.stopPropagation();
	}}
>
	<Combobox
		bind:value={value as string}
		{options}
		{placeholder}
		searchPlaceholder="Search models"
		emptyText={loading ? "Loading…" : "No match: press Enter to use what you typed"}
		creatable
		ariaLabel="Model"
		error={error || undefined}
		class={id}
	/>
</div>
