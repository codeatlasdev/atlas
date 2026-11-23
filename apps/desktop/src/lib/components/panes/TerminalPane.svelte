<script lang="ts">
	/** Body of a terminal tab: ended banner, then the GPU terminal or the xterm.js fallback. */
	import { IconButton } from "@codeatlasdev/ui";
	import { RotateCw } from "@lucide/svelte";
	import GpuTerminal from "#lib/components/GpuTerminal.svelte";
	import { errors } from "#lib/errors/errors.svelte.ts";
	import Terminal from "#lib/components/Terminal.svelte";
	import { gpu } from "#lib/stores/gpu.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import PaneNotice from "./PaneNotice.svelte";
	import { ws, type Tab } from "#lib/stores/workspace.svelte.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { fileTarget } from "#lib/termPath.ts";
	import { FileProbe } from "#lib/termPathExists.ts";
	import { host } from "#lib/host.ts";
	import { m } from "#lib/i18n/m.ts";

	/** `pty` is set: the caller only renders this for terminals that have one. */
	let { t, pty, active, focused }: { t: Tab; pty: string; active: boolean; focused: boolean } = $props();

	const ended = $derived(ws.ended(t));
	const onLink = (url: string, external: boolean) => openPreview(t.worktreeId, url, external);
	/** ⌘-click on `path:line:col` in the output: open it in the editor when it is a file of this worktree. */
	const onFile = (token: string) => {
		const root = ws.worktree(t.worktreeId)?.path;
		const hit = root ? fileTarget(token, t.cwd ?? null, root) : null;
		if (!hit) return false;
		openFile(t.worktreeId, hit.path, hit.line, hit.col);
		return true;
	};
	/** ⌘-hover lights a path only when it is an existing file of this worktree (a click needs only the shape). */
	const probe = new FileProbe((abs) => host.fsStat(abs));
	const isFile = (token: string) => {
		const root = ws.worktree(t.worktreeId)?.path;
		return root ? probe.isFile(token, t.cwd ?? null, root) : false;
	};
</script>

{#if t.chat && !ended}
	<PaneNotice onclick={() => (t.rawView = false)}>{m.workbench_term_pane_raw_protocol_back()}</PaneNotice>
{/if}
{#if ws.hostDown}
	<!-- The host did not answer: the terminal may well be alive, and Restart would fail the same way. -->
	<PaneNotice>
		<span>{m.workbench_term_pane_host_unreachable()}</span>
		<IconButton
			size="sm"
			variant="text"
			aria-label={m.workbench_term_pane_restart_host()}
			onclick={() => errors.restartHost()}
		>
			<RotateCw size={13} />
		</IconButton>
		<span class="hint">{m.workbench_term_pane_restart_host()}</span>
	</PaneNotice>
{:else if ended}
	<PaneNotice>
		<span
			>{ended === "gone"
				? m.workbench_term_pane_gone()
				: m.workbench_term_pane_ended({ code: ws.exited[pty] ?? "?" })}</span
		>
		<IconButton
			size="sm"
			variant="text"
			aria-label={m.settings_host_action_restart()}
			onclick={() => ws.restart(t.id)}
		>
			<RotateCw size={13} />
		</IconButton>
		<span class="hint">{m.settings_host_action_restart()}</span>
	</PaneNotice>
{/if}
{#if ended !== "gone"}
	{#key pty}
		{#if gpu.supported && gpu.terminal && !gpu.failed && !gpu.hostTooOld && !ws.worktree(t.worktreeId)?.hostId}
			<GpuTerminal
				tab={t.id}
				{pty}
				visible={active}
				focused={focused && active}
				readonly={ended !== null}
				{onLink}
				{onFile}
				{isFile}
			/>
		{:else}
			<Terminal {pty} visible={active} focused={focused && active} readonly={ended !== null} {onLink} />
		{/if}
	{/key}
{/if}

<style>
	.hint {
		color: var(--ca-text-secondary);
	}
</style>
