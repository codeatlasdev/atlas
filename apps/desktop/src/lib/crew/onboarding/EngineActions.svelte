<!-- Engine step buttons: back out of a sign-in, continue with the Ready engine, skip. -->
<script lang="ts">
	import type { Onboarding } from "./onboarding.svelte.ts";

	let { ob }: { ob: Onboarding } = $props();
</script>

<div class="action-group">
	{#if ob.signinActive}
		<button class="primary-btn" onclick={() => ob.closeSignin()}>Back</button>
	{/if}
	{#if !ob.signinActive && ob.readyEngine}
		<!-- After install/sign-in the engine is Ready: continue with it, no second click on the card. -->
		<button class="primary-btn" onclick={() => ob.continueWithReadyEngine()}>
			Continue with {ob.readyEngineLabel}
		</button>
	{/if}
	<button class="text-link" onclick={ob.finish}>Skip for now</button>
</div>
