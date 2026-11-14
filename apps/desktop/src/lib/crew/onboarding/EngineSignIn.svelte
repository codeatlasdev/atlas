<!-- Engine step right panel while an engine is being signed in: own-provider form, API key form or sign-in terminal. -->
<script lang="ts">
	import { Globe } from "@lucide/svelte";
	import ModelPicker from "#lib/crew/ModelPicker.svelte";
	import { AGENT_AUTH_TYPE, AGENT_KEY_LABEL } from "#lib/crew/remoteAgent.ts";
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();

	const agentId = $derived(ob.signinActive ?? "");
</script>

<div class="signin-panel">
	{#if ob.providerOpen}
		<!-- ── Provider form (use your own gateway) ── -->
		<div class="signin-key-form provider-form">
			<p class="signin-title">Use your own provider</p>
			<p class="signin-hint">Point {agentId} at OpenRouter, LiteLLM, or any compatible gateway.</p>
			<label class="field-label" for="prov-url">Base URL</label>
			<input
				id="prov-url"
				class="ssh-input"
				type="url"
				placeholder="https://openrouter.ai/api/v1"
				bind:value={ob.provider.baseUrl}
				aria-label="Gateway base URL"
			/>
			{#if agentId !== "claude"}
				<label class="field-label" for="prov-fmt">API format</label>
				<select id="prov-fmt" class="provider-select" bind:value={ob.provider.format} aria-label="API format">
					<option value="openai">OpenAI-compatible (Chat Completions)</option>
					<option value="anthropic">Anthropic-compatible (Messages API)</option>
				</select>
				{#if agentId === "codex"}
					<p class="field-hint">
						Codex only supports gateways that speak the OpenAI Responses API (/v1/responses), not Chat
						Completions.
					</p>
				{/if}
			{:else}
				<p class="field-hint">
					Claude Code only speaks the Anthropic Messages API. Your gateway must be Anthropic-compatible.
				</p>
			{/if}
			<label class="field-label" for="prov-key">API key</label>
			<input
				id="prov-key"
				class="ssh-input"
				type="password"
				placeholder="sk-…"
				bind:value={ob.provider.apiKey}
				aria-label="Gateway API key"
			/>
			<span class="field-label">Model</span>
			<ModelPicker baseUrl={ob.provider.baseUrl} apiKey={ob.provider.apiKey} bind:value={ob.provider.model} />
			{#if ob.provider.testResult}
				<p
					class="field-hint"
					style="color: {ob.provider.testResult.ok
						? 'var(--ca-semantic-success)'
						: 'var(--ca-danger, #ef4444)'}"
				>
					{ob.provider.testResult.text}
				</p>
			{/if}
			{#if ob.provider.saveError}
				<p class="field-error" role="alert">{ob.provider.saveError}</p>
			{/if}
			<div class="provider-actions">
				<button
					class="primary-btn"
					onclick={() => ob.saveProvider()}
					disabled={ob.provider.saving || !ob.provider.canSave}
				>
					{ob.provider.saving ? "Saving…" : "Save"}
				</button>
				<button
					class="text-link"
					onclick={() => {
						if (!ob.provider.testResult?.ok) void ob.provider.test();
					}}
					disabled={ob.provider.testing || !ob.provider.canTest}
				>
					{ob.provider.testing ? "Testing…" : "Test connection"}
				</button>
			</div>
		</div>
	{:else if ob.apiKeyMode}
		<div class="signin-key-form">
			<p class="signin-title">Paste your {AGENT_KEY_LABEL[agentId] ?? "API key"}</p>
			<p class="signin-hint">It's stored on your crew server only, never sent anywhere else.</p>
			<input
				class="ssh-input"
				type="password"
				placeholder="sk-…"
				bind:value={ob.apiKeyValue}
				onkeydown={(e) => e.key === "Enter" && ob.applyApiKey()}
				aria-label="API key"
			/>
			{#if ob.apiKeyError}<p class="field-error">{ob.apiKeyError}</p>{/if}
			<button
				class="primary-btn"
				onclick={() => ob.applyApiKey()}
				disabled={ob.apiKeyApplying || !ob.apiKeyValue.trim()}
			>
				{ob.apiKeyApplying ? "Applying…" : "Apply"}
			</button>
		</div>
	{:else}
		<div class="signin-terminal">
			<div class="signin-term-output" aria-live="polite" aria-label="Terminal output">
				<pre>{ob.signinOutput || "Connecting…"}</pre>
			</div>
			<div class="signin-term-input-row">
				<input
					class="signin-term-input"
					type="text"
					placeholder="Type here and press Enter…"
					bind:value={ob.signinInput}
					onkeydown={(e) => ob.sendSigninKey(e)}
					aria-label="Terminal input"
				/>
			</div>
		</div>
	{/if}
	<!-- Toggle between modes -->
	{#if !ob.providerOpen}
		{#if AGENT_AUTH_TYPE[agentId] === "both"}
			<button class="text-link" onclick={() => ob.toggleApiKeyMode()} style="font-size:12px;margin-top:8px">
				{ob.apiKeyMode ? "Use interactive sign-in instead" : "Paste API key instead"}
			</button>
		{/if}
		<button class="text-link" onclick={() => ob.openProviderMode()} style="font-size:12px;margin-top:4px">
			<Globe size={11} style="display:inline;vertical-align:middle;margin-right:4px" />Use your own provider
		</button>
		<button class="text-link" onclick={() => ob.recheckSignin()} style="font-size:12px">Check again</button>
	{:else}
		<button class="text-link" onclick={() => ob.closeProviderMode()} style="font-size:12px;margin-top:8px">
			Use direct key instead
		</button>
	{/if}
</div>

<style>
	.signin-panel {
		width: min(380px, 90%);
		display: flex;
		flex-direction: column;
		gap: 10px;
		align-items: flex-start;
	}
	.signin-title {
		font-size: 15px;
		font-weight: 600;
		color: var(--ca-text-primary);
		margin: 0;
	}
	.signin-hint {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0 0 4px;
	}
	.signin-key-form {
		display: flex;
		flex-direction: column;
		gap: 10px;
		width: 100%;
	}
	.signin-terminal {
		width: 100%;
		display: flex;
		flex-direction: column;
		border: 1.5px solid var(--ca-border-subtle);
		border-radius: 8px;
		overflow: hidden;
	}
	.signin-term-output {
		background: var(--ca-surface-sunken);
		min-height: 160px;
		max-height: 260px;
		overflow-y: auto;
		padding: 10px 12px;
	}
	.signin-term-output pre {
		font-family: monospace;
		font-size: 11.5px;
		line-height: 1.6;
		color: var(--ca-text-primary);
		white-space: pre-wrap;
		word-break: break-all;
		margin: 0;
	}
	.signin-term-input-row {
		border-top: 1px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
	}
	.signin-term-input {
		width: 100%;
		box-sizing: border-box;
		padding: 8px 12px;
		background: none;
		border: none;
		outline: none;
		font-family: monospace;
		font-size: 12px;
		color: var(--ca-text-primary);
	}
	.provider-form {
		gap: 8px;
	}
	.provider-select {
		width: 100%;
		box-sizing: border-box;
		padding: 9px 12px;
		border-radius: 8px;
		border: 1.5px solid var(--ca-border-subtle);
		background: var(--ca-surface-base);
		color: var(--ca-text-primary);
		font-size: 14px;
		outline: none;
		transition: border-color 150ms;
		appearance: auto;
	}
	.provider-select:focus {
		border-color: var(--ca-accent, #4f8ef7);
	}
	.provider-actions {
		display: flex;
		gap: 10px;
		align-items: center;
		flex-wrap: wrap;
	}
	.provider-actions :global(.primary-btn) {
		height: 40px;
		padding: 0 24px;
		font-size: 14px;
	}
</style>
