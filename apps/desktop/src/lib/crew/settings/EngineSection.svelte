<!-- Engine: which coding agent runs the crewmate, and the optional custom provider (own API gateway). -->
<script lang="ts">
	import { Button, toast } from "@codeatlasdev/ui";
	import { Globe } from "@lucide/svelte";
	import type { Crewmate } from "#lib/crew.ts";
	import ModelPicker from "#lib/crew/ModelPicker.svelte";
	import type { ProviderForm } from "#lib/crew/providerForm.svelte.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { crewmate, agentLabel, provider }: { crewmate: Crewmate; agentLabel: string; provider: ProviderForm } =
		$props();
</script>

<section class="card rows">
	<div class="row">
		<div class="row-text">
			<span class="row-label">{m.crew_settings_crewmate_dialog_engine()}</span>
			<span class="row-hint"
				>{m.crew_settings_engine_section_the_coding_agent_that_runs({ name: crewmate.name })}</span
			>
		</div>
		<span class="row-value">{agentLabel}</span>
	</div>
</section>

<!-- The provider is one unit (files written on the crewmate's crew server + a connection check), so it keeps an
     explicit Save and Test instead of saving field by field. -->
<section class="card">
	<div class="card-head">
		<span class="row-label"
			><Globe size={13} aria-hidden="true" /> {m.crew_settings_engine_section_custom_provider()}</span
		>
	</div>
	<p class="row-hint">{m.crew_settings_engine_section_optional_route_this_engine_through()}</p>
	<div class="form-grid">
		<label for="bsd-prov-url">{m.crew_settings_engine_section_base_url()}</label>
		<input
			id="bsd-prov-url"
			class="field-input"
			type="url"
			placeholder="https://openrouter.ai/api/v1"
			bind:value={provider.baseUrl}
		/>
		{#if crewmate.agent !== "claude"}
			<label for="bsd-prov-fmt">{m.crew_settings_engine_section_api_format()}</label>
			<select id="bsd-prov-fmt" class="field-select" bind:value={provider.format}>
				<option value="openai">{m.crew_settings_engine_section_openai_compatible()}</option>
				<option value="anthropic">{m.crew_settings_engine_section_anthropic_compatible()}</option>
			</select>
		{/if}
		<label for="bsd-prov-key">{m.crew_settings_engine_section_api_key()}</label>
		<input
			id="bsd-prov-key"
			class="field-input"
			type="password"
			placeholder="sk-…"
			bind:value={provider.apiKey}
		/>
		<span class="form-label">{m.crew_settings_engine_section_model()}</span>
		<ModelPicker baseUrl={provider.baseUrl} apiKey={provider.apiKey} bind:value={provider.model} />
	</div>
	{#if provider.testResult}
		<p class="provider-result" class:ok={provider.testResult.ok}>{provider.testResult.text}</p>
	{/if}
	{#if provider.saveError}
		<p class="provider-save-error" role="alert">{provider.saveError}</p>
	{/if}
	<div class="pane-actions">
		<Button size="sm" variant="ghost" disabled={!provider.canTest} onclick={() => void provider.test()}>
			{provider.testing
				? m.crew_settings_engine_section_testing()
				: m.crew_settings_engine_section_test_connection()}
		</Button>
		<Button
			size="sm"
			variant="outlined"
			disabled={!provider.canSave}
			onclick={async () => {
				if (await provider.save(crew.selected?.hostKey, crewmate.agent))
					toast.success(m.crew_settings_engine_section_provider_configured());
			}}
		>
			{provider.saving ? m.common_state_saving() : m.crew_settings_engine_section_save_provider()}
		</Button>
	</div>
</section>

<style>
	.form-grid {
		display: grid;
		grid-template-columns: 96px 1fr;
		align-items: center;
		gap: 10px 14px;
		margin-top: 6px;
	}
	.form-grid > label,
	.form-label {
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.provider-result {
		font-size: 12px;
		margin: 0;
		color: var(--ca-semantic-error, #ef4444);
	}
	.provider-result.ok {
		color: var(--ca-semantic-success, #22c55e);
	}
	.provider-save-error {
		font-size: 12px;
		margin: 0;
		color: var(--ca-semantic-error, #ef4444);
	}
</style>
