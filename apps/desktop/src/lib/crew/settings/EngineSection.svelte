<!-- Engine: which coding agent runs the crewmate, and the optional custom provider (own API gateway). -->
<script lang="ts">
	import { Button, toast } from "@codeatlasdev/ui";
	import { Globe } from "@lucide/svelte";
	import type { Crewmate } from "#lib/crew.ts";
	import ModelPicker from "#lib/crew/ModelPicker.svelte";
	import type { ProviderForm } from "#lib/crew/providerForm.svelte.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";

	let { crewmate, agentLabel, provider }: { crewmate: Crewmate; agentLabel: string; provider: ProviderForm } =
		$props();
</script>

<section class="card rows">
	<div class="row">
		<div class="row-text">
			<span class="row-label">Engine</span>
			<span class="row-hint">The coding agent that runs {crewmate.name}'s tasks.</span>
		</div>
		<span class="row-value">{agentLabel}</span>
	</div>
</section>

<!-- The provider is one unit (files written on the crewmate's crew server + a connection check), so it keeps an
     explicit Save and Test instead of saving field by field. -->
<section class="card">
	<div class="card-head">
		<span class="row-label"><Globe size={13} aria-hidden="true" /> Custom provider</span>
	</div>
	<p class="row-hint">Optional. Route this engine through your own API gateway.</p>
	<div class="form-grid">
		<label for="bsd-prov-url">Base URL</label>
		<input
			id="bsd-prov-url"
			class="field-input"
			type="url"
			placeholder="https://openrouter.ai/api/v1"
			bind:value={provider.baseUrl}
		/>
		{#if crewmate.agent !== "claude"}
			<label for="bsd-prov-fmt">API format</label>
			<select id="bsd-prov-fmt" class="field-select" bind:value={provider.format}>
				<option value="openai">OpenAI-compatible</option>
				<option value="anthropic">Anthropic-compatible</option>
			</select>
		{/if}
		<label for="bsd-prov-key">API key</label>
		<input
			id="bsd-prov-key"
			class="field-input"
			type="password"
			placeholder="sk-…"
			bind:value={provider.apiKey}
		/>
		<span class="form-label">Model</span>
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
			{provider.testing ? "Testing…" : "Test connection"}
		</Button>
		<Button
			size="sm"
			variant="outlined"
			disabled={!provider.canSave}
			onclick={async () => {
				if (await provider.save(crew.selected?.hostKey, crewmate.agent)) toast.success("Provider configured");
			}}
		>
			{provider.saving ? "Saving…" : "Save provider"}
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
