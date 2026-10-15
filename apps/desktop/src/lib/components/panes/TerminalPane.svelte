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
</script>

{#if t.chat && !ended}
	<PaneNotice onclick={() => (t.rawView = false)}>Raw omp protocol · back to chat</PaneNotice>
{/if}
{#if ws.hostDown}
	<!-- The host did not answer: the terminal may well be alive, and Restart would fail the same way. -->
	<PaneNotice>
		<span>Terminal host not reachable</span>
		<IconButton
			size="sm"
			variant="text"
			aria-label="Restart terminal host"
			onclick={() => errors.restartHost()}
		>
			<RotateCw size={13} />
		</IconButton>
		<span class="hint">Restart terminal host</span>
	</PaneNotice>
{:else if ended}
	<PaneNotice>
		<span>{ended === "gone" ? "Terminal is gone" : `Process ended (${ws.exited[pty] ?? "?"})`}</span>
		<IconButton size="sm" variant="text" aria-label="Restart" onclick={() => ws.restart(t.id)}>
			<RotateCw size={13} />
		</IconButton>
		<span class="hint">Restart</span>
	</PaneNotice>
{/if}
{#if ended !== "gone"}
	{#key pty}
		{#if gpu.supported && gpu.terminal && !gpu.hostTooOld && !ws.worktree(t.worktreeId)?.hostId}
			<GpuTerminal
				tab={t.id}
				{pty}
				visible={active}
				focused={focused && active}
				readonly={ended !== null}
				{onLink}
				{onFile}
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
