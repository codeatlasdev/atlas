<!-- Connect step buttons: create key / authorize / retry with sudo password / add crew server / skip. -->
<script lang="ts">
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();
</script>

<div class="action-group">
	{#if !ob.keyChecked}
		<!-- waiting for key check -->
	{:else if !ob.hasKey}
		<button class="primary-btn" onclick={() => ob.createSshKey()} disabled={ob.creatingKey}>
			{ob.creatingKey ? "Creating…" : "Create SSH key"}
		</button>
	{:else if ob.authMode === "authorize"}
		<button
			class="primary-btn"
			onclick={() => ob.authorizeAndRetry()}
			disabled={ob.authRunning || !ob.authPassword.trim()}
		>
			{ob.authRunning ? "Authorizing…" : "Authorize this Mac"}
		</button>
		<button class="text-link" onclick={() => ob.cancelAuth()}>Cancel</button>
	{:else if ob.authMode === "sudo"}
		<button
			class="primary-btn"
			onclick={() => ob.launchServer(ob.authPassword)}
			disabled={ob.installRunning || !ob.authPassword.trim()}
		>
			{ob.installRunning ? "Installing…" : "Retry with password"}
		</button>
		<button class="text-link" onclick={() => ob.cancelAuth()}>Cancel</button>
	{:else}
		<button
			class="primary-btn"
			onclick={() => ob.launchServer()}
			disabled={ob.installRunning || ob.installDone || !ob.targetValid}
		>
			{ob.installRunning ? "Installing…" : "Add crew server"}
		</button>
		{#if ob.failedPhase && !ob.installRunning && !ob.installDone}
			<button class="text-link" onclick={() => ob.launchServer()}>Retry</button>
		{/if}
		{#if !ob.installRunning && !ob.installDone}
			<button class="text-link" onclick={ob.advance}>Skip for now</button>
		{/if}
	{/if}
</div>
