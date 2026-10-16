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
	import { useSpace } from "#lib/shell/spaceCtx.ts";

	const space = useSpace();
	let p = $derived(space.project);
	// "Not now" hides the omp prompt on this computer until the integration is enabled from Settings.
	const OMP_DISMISSED = "atlas.ompPromptDismissed";
	let ompDismissed = $state(localStorage.getItem(OMP_DISMISSED) === "1");
	function dismissOmp() {
		localStorage.setItem(OMP_DISMISSED, "1");
		ompDismissed = true;
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
	{#if agents.ompIntegration === "missing" && !ompDismissed}
		<div use:corner class="floating">
			<Alert severity="info" title="Enable omp integration">
				Atlas shows omp's state (working, needs you, done) through a small omp extension.
				<Button size="sm" onclick={() => agents.enableOmp()}>Enable</Button>
				<Button size="sm" variant="ghost" onclick={dismissOmp}>Not now</Button>
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
