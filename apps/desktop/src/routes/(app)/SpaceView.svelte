<script lang="ts">
	/**
	 * One space's page: header, drawers, banners and the workbench. Mounted for the current space
	 * and its neighbours; everything below reads its worktree/project from `useSpace()`.
	 */
	import { Alert, Button } from "@codeatlasdev/ui";
	import Contain from "#lib/errors/Contain.svelte";
	import ActivityDrawer from "#lib/panel/ActivityDrawer.svelte";
	import { corner } from "#lib/shell/cornerStack.ts";
	import ConfirmAction from "./ConfirmAction.svelte";
	import MissingProject from "./MissingProject.svelte";
	import ProjectHeader from "./ProjectHeader.svelte";
	import Workbench from "./Workbench.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { dismissedKey, promptFor } from "#lib/stores/agentIntegration.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";

	const space = useSpace();
	let p = $derived(space.project);
	// One agent's integration at a time. "Not now" hides it on this computer; Settings → Integrations → Agent hooks
	// still turns it on.
	let hidden = $state(new Set<string>());
	let offer = $derived(
		promptFor(agents.integrations, (id) => hidden.has(id) || localStorage.getItem(dismissedKey(id)) === "1"),
	);
	function dismiss(id: string) {
		localStorage.setItem(dismissedKey(id), "1");
		hidden = new Set([...hidden, id]);
	}
</script>

{#if p?.missing}
	<MissingProject {p} />
{:else if p}
	<ProjectHeader {p} />
	{#if space.current}<ActivityDrawer projectId={p.id} />{/if}

	{#if p.error}
		<div class="banner">
			<Alert severity="error" title="atlas.yaml has problems"
				><pre class="mono ca-selectable">{p.error}</pre></Alert
			>
		</div>
	{/if}

	<div class="rule"></div>
	{#if offer}
		{@const o = offer}
		<div use:corner class="floating">
			<Alert severity="info" title="Enable {o.label} integration">
				Atlas and Atlas Mobile show {o.label}'s state (working, needs you, done) through its hooks.
				<Button size="sm" onclick={() => agents.enableIntegration(o.id)}>Enable</Button>
				<Button size="sm" variant="ghost" onclick={() => dismiss(o.id)}>Not now</Button>
			</Alert>
		</div>
	{/if}

	<Contain
		where="Workbench"
		title="The workspace hit an error"
		hint="The sidebar and project header still work."
	>
		<Workbench />
	</Contain>
{/if}

{#if space.current}<ConfirmAction />{/if}

<style>
	.banner {
		padding: 0 20px 10px;
	}
	/* Found by a probe that finishes after the first frame: it lives in the corner stack (above the Getting started
	   card, never over it) instead of pushing the workbench (and the native terminal views laid over it) down. */
	.floating {
		width: 100%;
		box-shadow: 0 8px 28px rgb(0 0 0 / 0.28);
		border-radius: 12px;
	}
	.banner pre {
		margin: 4px 0 0;
		font-size: 11.5px;
		white-space: pre-wrap;
	}
	.rule {
		border-bottom: 0.5px solid var(--ca-outline-subtle);
	}
</style>
