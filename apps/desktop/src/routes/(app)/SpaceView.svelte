<script lang="ts">
	/**
	 * One space's page: header, drawers, banners and the workbench. Mounted for the current space
	 * and its neighbours; everything below reads its worktree/project from `useSpace()`.
	 */
	import { Alert, Button } from "@codeatlasdev/ui";
	import Contain from "#lib/errors/Contain.svelte";
	import ActivityDrawer from "#lib/panel/ActivityDrawer.svelte";
	import MachineDrawer from "#lib/panel/MachineDrawer.svelte";
	import CornerNotice from "#lib/shell/CornerNotice.svelte";
	import ConfirmAction from "./ConfirmAction.svelte";
	import MissingProject from "./MissingProject.svelte";
	import ProjectHeader from "./ProjectHeader.svelte";
	import Workbench from "./Workbench.svelte";
	import { agents } from "#lib/stores/agents.svelte.ts";
	import { dismissedKey, promptFor } from "#lib/stores/agentIntegration.ts";
	import { useSpace } from "#lib/shell/spaceCtx.ts";
	import { m } from "#lib/i18n/m.ts";

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
	{#if space.current && p.kind !== "folder"}<ActivityDrawer projectId={p.id} /><MachineDrawer />{/if}

	{#if p.error}
		<div class="banner">
			<Alert severity="error" title={m.shell_space_view_config_problems()}>
				<pre class="mono ca-selectable">{p.error}</pre>
			</Alert>
		</div>
	{/if}

	<div class="rule"></div>
	<!-- A suggestion in the corner column: it waits behind errors and toasts, one at a time, and sits in the column
	     instead of pushing the workbench (and the native terminal views laid over it) down. Only the space on screen
	     may offer it, or every mounted space adds its own copy (three projects open = three identical cards). -->
	{#if offer && space.current}
		{@const o = offer}
		{#snippet act()}
			<Button size="sm" onclick={() => agents.enableIntegration(o.id)}>
				{m.shell_space_view_integration_enable()}
			</Button>
			<Button size="sm" variant="ghost" onclick={() => dismiss(o.id)}>
				{m.shell_space_view_integration_not_now()}
			</Button>
		{/snippet}
		<CornerNotice
			id="integration:{o.id}"
			kind="suggestion"
			tone="info"
			title={m.shell_space_view_integration_title({ label: o.label })}
			body={m.shell_space_view_integration_body({ label: o.label })}
			actions={act}
			ondismiss={() => dismiss(o.id)}
		/>
	{/if}

	<Contain where="Workbench" title={m.shell_space_view_title()} hint={m.shell_space_view_hint()}>
		<Workbench />
	</Contain>
{/if}

{#if space.current}<ConfirmAction />{/if}

<style>
	.banner {
		padding: 0 20px 10px;
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
