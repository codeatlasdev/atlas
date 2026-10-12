<script lang="ts">
	import { Alert, Button, Spinner, toast } from "@codeatlasdev/ui";
	import { Check, ChevronRight, RefreshCw, X } from "@lucide/svelte";
	import { api, errText, type ShellInfo } from "#lib/api.ts";
	import { tilde } from "#lib/format.ts";

	let info = $state<ShellInfo | null>(null);
	let busy = $state(false);
	let showPath = $state(false);
	let custom = $state("");
	let error = $state<string | null>(null);

	const SOURCE = {
		settings: "picked in Settings",
		account: "your Mac account's login shell",
		$SHELL: "taken from $SHELL",
		default: "built-in fallback",
	} as const;

	$effect(() => {
		api
			.shellInfo()
			.then((i) => (info = i))
			.catch((e) => (error = errText(e)));
	});

	let options = $derived(
		info ? [...new Set([...info.installed, ...(info.configured ? [info.configured] : [])])] : [],
	);
	let found = $derived(info?.tools.filter(([, p]) => p).length ?? 0);

	async function set(shell: string | null) {
		busy = true;
		try {
			info = await api.shellSet(shell);
			if (info.probe?.error) toast.error(`Atlas could not start ${info.probe.shell}`);
			else toast.success("Shell reloaded. Stop and start running projects to use it.");
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}
</script>

{#if error}
	<Alert severity="error" title="Atlas could not read your shell">{error}</Alert>
{:else if !info}
	<div class="muted"><Spinner size={12} /> Reading your shell…</div>
{:else}
	<div class="group">
		<div class="field">
			<div class="flabel">Shell<small>Atlas runs this to learn your PATH.</small></div>
			<div class="control">
				<select
					class="mono"
					value={info.configured ?? "auto"}
					disabled={busy}
					onchange={(e) => {
						const v = e.currentTarget.value;
						set(v === "auto" ? null : v);
					}}
				>
					<option value="auto"
						>Automatic (recommended){info.probe && !info.configured
							? ` — ${info.probe.shell.split("/").pop()}`
							: ""}</option
					>
					{#each options as s (s)}<option value={s}>{s}</option>{/each}
				</select>
				<Button variant="ghost" size="sm" disabled={busy} onclick={() => set(info?.configured ?? null)}>
					{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<RefreshCw size={13} />{/if}{/snippet}
					Reload now
				</Button>
			</div>
		</div>
		<form
			class="field"
			onsubmit={(e) => {
				e.preventDefault();
				if (custom.trim()) set(custom.trim());
			}}
		>
			<div class="flabel">Other shell<small>Full path to a shell not listed.</small></div>
			<div class="control">
				<input class="mono" placeholder="/usr/local/bin/nu" bind:value={custom} disabled={busy} />
				<Button variant="glass" size="sm" type="submit" disabled={busy || !custom.trim()}
					>Use this shell</Button
				>
			</div>
		</form>
		<div class="status">
			{#if info.probe?.error}
				<span class="st bad"
					><X size={13} /> Could not start <span class="mono">{info.probe.shell}</span></span
				>
			{:else if info.probe}
				<span class="st ok"><Check size={13} /> Using <span class="mono">{info.probe.shell}</span></span>
				<span class="muted"
					>{SOURCE[info.probe.source]}, <span class="mono">{info.probe.flags}</span>, {info.probe.took_ms} ms</span
				>
			{/if}
		</div>
	</div>

	{#if info.probe?.error}
		<Alert severity="error" title="Your projects only see the basic system PATH">
			Tools such as bun or node may not be found. Pick another shell, or fix the error below in your shell
			startup files, then reload.
			<div class="mono err">{info.probe.error}</div>
		</Alert>
	{/if}

	<div class="group">
		<div class="ghead">
			<span>Tools your projects can find</span>
			<span class="muted">{found} of {info.tools.length} found on PATH</span>
		</div>
		<div class="tools">
			{#each info.tools as [tool, path] (tool)}
				<div class="tool" class:miss={!path}>
					<span class="tic"
						>{#if path}<Check size={12} />{:else}<X size={12} />{/if}</span
					>
					<span class="tname">{tool}</span>
					<span class="tpath mono" title={path ?? ""}>{path ? tilde(path) : "not found"}</span>
				</div>
			{/each}
		</div>
		{#if info.path.length}
			<button class="toggle" onclick={() => (showPath = !showPath)}>
				<ChevronRight
					size={13}
					style="transform: rotate({showPath ? 90 : 0}deg); transition: transform 150ms"
				/>
				Show the full PATH ({info.path.length} folders)
			</button>
			{#if showPath}<pre class="mono selectable">{info.path.join("\n")}</pre>{/if}
		{/if}
	</div>
{/if}

<style>
	.group {
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
	}
	.field {
		display: grid;
		grid-template-columns: 170px minmax(0, 1fr);
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		margin: 0;
	}
	.field + .field {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.flabel {
		font-size: 13px;
		font-weight: 500;
	}
	.flabel small {
		display: block;
		margin-top: 2px;
		font-size: 11.5px;
		font-weight: 400;
		line-height: 1.35;
		color: var(--ca-text-secondary);
	}
	.control {
		display: flex;
		gap: 8px;
		min-width: 0;
	}
	select,
	input {
		flex: 1;
		min-width: 0;
		height: 30px;
		font-size: 12px;
		padding: 0 8px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		color: var(--ca-text-primary);
	}
	.status {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		border-radius: 0 0 10px 10px;
		font-size: 12px;
	}
	.st {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-weight: 600;
	}
	.st.ok {
		color: var(--ca-semantic-success);
	}
	.st.bad {
		color: var(--ca-semantic-error);
	}
	.st .mono {
		color: var(--ca-text-primary);
		font-weight: 500;
	}
	.muted {
		font-size: 12px;
		color: var(--ca-text-secondary);
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.err {
		margin-top: 6px;
		font-size: 11px;
		white-space: pre-wrap;
	}
	.ghead {
		display: flex;
		justify-content: space-between;
		padding: 10px 14px 4px;
		font-size: 13px;
		font-weight: 500;
	}
	.tools {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		padding: 4px 8px 8px;
	}
	.tool {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 6px;
		font-size: 12.5px;
		min-width: 0;
	}
	.tic {
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--ca-semantic-success) 14%, transparent);
		flex-shrink: 0;
	}
	.tool.miss .tic {
		color: var(--ca-text-disabled);
		background: var(--ca-surface-sunken);
	}
	.tname {
		font-weight: 600;
		width: 52px;
		flex-shrink: 0;
	}
	.tool.miss .tname {
		color: var(--ca-text-secondary);
		font-weight: 500;
	}
	.tpath {
		font-size: 11px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.toggle {
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		border: 0;
		border-top: 0.5px solid var(--ca-outline-subtle);
		background: none;
		padding: 9px 12px;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.toggle:hover {
		color: var(--ca-text-primary);
	}
	pre {
		margin: 0 12px 12px;
		max-height: 160px;
		overflow: auto;
		font-size: 11px;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--ca-surface-sunken);
	}
</style>
