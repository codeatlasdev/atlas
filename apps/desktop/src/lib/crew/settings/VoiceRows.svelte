<!-- Dictation on one crew server: its speech engine and state, and the owner's crew-wide switch. -->
<script lang="ts">
	import { onMount } from "svelte";
	import { Switch, toast } from "@codeatlasdev/ui";
	import { errText } from "#lib/api.ts";
	import Row from "#lib/settings/Row.svelte";
	import { voiceStatus } from "#lib/crew/voice/status.svelte.ts";
	import { voiceStatusText } from "#lib/crew/voice/statusText.ts";
	import { m } from "#lib/i18n/m.ts";

	let { hostKey }: { hostKey: string } = $props();

	let saving = $state(false);
	const status = $derived(voiceStatus.loaded[hostKey] ? voiceStatus.of(hostKey) : undefined);

	onMount(() => {
		void voiceStatus.load(hostKey);
	});

	async function toggle(enabled: boolean) {
		saving = true;
		try {
			await voiceStatus.set(hostKey, { enabled });
		} catch (e) {
			// Not the owner: hostd says so with its own coded sentence.
			toast.error(errText(e));
			void voiceStatus.load(hostKey);
		} finally {
			saving = false;
		}
	}
</script>

<Row
	id="crew-voice-{hostKey}"
	label={m.crew_voice_settings_status()}
	description={m.crew_voice_settings_status_hint()}
>
	<span class="state" data-voice-status>{voiceStatusText(status)}</span>
</Row>
{#if status}
	<Row
		id="crew-voice-switch-{hostKey}"
		label={m.crew_voice_switch()}
		description={m.crew_voice_switch_hint()}
	>
		<Switch
			aria-label={m.crew_voice_switch()}
			checked={status.enabled}
			disabled={saving}
			onchange={(v) => void toggle(v)}
		/>
	</Row>
{/if}

<style>
	.state {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
