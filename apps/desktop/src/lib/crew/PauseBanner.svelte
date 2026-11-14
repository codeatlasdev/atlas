<script lang="ts">
	/**
	 * PauseBanner — shown over the Crew workspace while any crew server is paused. One line, one button per paused
	 * server: pausing cancelled the running tasks and holds the queues, so the person needs to see why nothing moves.
	 */
	import { Button, toast } from "@codeatlasdev/ui";
	import { Pause } from "@lucide/svelte";
	import { errText } from "#lib/api.ts";
	import { crewServerLabel } from "#lib/crew/sshTarget.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";

	const paused = $derived(crew.servers.filter((h) => h.paused === true));
	let busy = $state<string | null>(null);

	async function resume(key: string) {
		busy = key;
		try {
			await crew.resumeServer(key);
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = null;
		}
	}
</script>

{#each paused as h (h.key)}
	<div class="pause-banner" role="status">
		<Pause size={13} aria-hidden="true" />
		<span class="msg">
			Paused{crew.servers.length > 1 ? `: ${crewServerLabel(h.name)}` : ""}. Running tasks were cancelled;
			queued tasks wait until you resume.
		</span>
		<Button size="sm" onclick={() => void resume(h.key)} disabled={busy === h.key}>
			{busy === h.key ? "Resuming…" : "Resume"}
		</Button>
	</div>
{/each}

<style>
	.pause-banner {
		flex: none;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		font-size: 12px;
		color: var(--ca-text-primary);
		background: var(--ca-surface-sunken);
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
	.msg {
		flex: 1;
		min-width: 0;
		color: var(--ca-text-secondary);
	}
</style>
