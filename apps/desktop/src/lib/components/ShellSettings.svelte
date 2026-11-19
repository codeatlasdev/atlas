<script lang="ts">
	import { Alert, Button, Select, Spinner, toast } from "@codeatlasdev/ui";
	import type { Snippet } from "svelte";
	import { Check, ChevronRight, RefreshCw, X } from "@lucide/svelte";
	import { api, errText, type ShellInfo } from "#lib/api.ts";
	import { tilde } from "#lib/format.ts";
	import FieldInput from "#lib/settings/FieldInput.svelte";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { m } from "#lib/i18n/m.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import { sourceText } from "#lib/settings/shellSource.ts";

	let info = $state<ShellInfo | null>(null);
	let busy = $state(false);
	let showPath = $state(false);
	let custom = $state("");
	let error = $state<string | null>(null);

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
			if (info.probe?.error)
				toast.error(m.settings_terminal_shell_could_not_start_toast({ shell: info.probe.shell }));
			else toast.success(m.settings_terminal_shell_reloaded());
		} catch (e) {
			toast.error(errText(e));
		} finally {
			busy = false;
		}
	}
</script>

{#snippet mono(content: Snippet)}<span class="mono">{@render content()}</span>{/snippet}

{#if error}
	<Alert severity="error" title={m.settings_terminal_shell_atlas_could_not_read_your()}>{error}</Alert>
{:else if !info}
	<div class="muted"><Spinner size={12} /> {m.settings_terminal_shell_reading_your_shell()}</div>
{:else}
	<Group>
		<Row
			label={m.settings_terminal_shell_shell()}
			description={m.settings_terminal_shell_atlas_runs_this_to_learn()}
		>
			<div class="select">
				<Select
					size="sm"
					value={info.configured ?? "auto"}
					disabled={busy}
					options={[
						{
							value: "auto",
							label: `Automatic (recommended)${
								info.probe && !info.configured ? ` — ${info.probe.shell.split("/").pop()}` : ""
							}`,
						},
						...options.map((o) => ({ value: o, label: o })),
					]}
					onchange={(v) => set(v === "auto" ? null : v)}
				/>
			</div>
			<Button variant="outlined" size="sm" disabled={busy} onclick={() => set(info?.configured ?? null)}>
				{#snippet leading()}{#if busy}<Spinner size={13} />{:else}<RefreshCw size={13} />{/if}{/snippet}
				{m.settings_terminal_shell_reload_now()}
			</Button>
		</Row>
		<Row
			label={m.settings_terminal_shell_other_shell()}
			description={m.settings_terminal_shell_full_path_to_a_shell()}
		>
			<form
				class="other"
				onsubmit={(e) => {
					e.preventDefault();
					if (custom.trim()) set(custom.trim());
				}}
			>
				<!-- i18n-ignore: an example path, not prose -->
				<FieldInput
					class="text mono"
					aria-label={m.settings_terminal_shell_other_shell()}
					placeholder="/usr/local/bin/nu"
					spellcheck="false"
					value={custom}
					oninput={(e) => (custom = e.currentTarget.value)}
					disabled={busy}
				/>
				<Button variant="outlined" size="sm" type="submit" disabled={busy || !custom.trim()}
					>{m.settings_terminal_shell_use_this_shell()}</Button
				>
			</form>
		</Row>
		<div class="status">
			{#if info.probe?.error}
				<span class="st bad"
					><X size={13} />
					<Rich
						parts={m.settings_terminal_shell_status_failed.parts({ shell: info.probe.shell })}
						tags={{ mono }}
					/></span
				>
			{:else if info.probe}
				<span class="st ok"
					><Check size={13} />
					<Rich
						parts={m.settings_terminal_shell_status_using.parts({ shell: info.probe.shell })}
						tags={{ mono }}
					/></span
				>
				<span class="muted"
					><Rich
						parts={m.settings_terminal_shell_status_detail.parts({
							source: sourceText(info.probe.source),
							flags: info.probe.flags ?? "",
							ms: info.probe.took_ms,
						})}
						tags={{ mono }}
					/></span
				>
			{/if}
		</div>
	</Group>

	{#if info.probe?.error}
		<Alert severity="error" title={m.settings_terminal_shell_your_projects_only_see_the()}>
			{m.settings_terminal_shell_tools_such_as_bun_or()}
			<div class="mono err">{info.probe.error}</div>
		</Alert>
	{/if}

	<Group>
		<div class="ghead">
			<span>{m.settings_terminal_shell_tools_your_projects_can_find()}</span>
			<span class="muted"
				>{m.settings_terminal_shell_found_of_length_found_on({ found, length: info.tools.length })}</span
			>
		</div>
		<div class="tools">
			{#each info.tools as [tool, path] (tool)}
				<div class="tool" class:miss={!path}>
					<span class="tic"
						>{#if path}<Check size={12} />{:else}<X size={12} />{/if}</span
					>
					<span class="tname">{tool}</span>
					<span class="tpath mono" title={path ?? ""}
						>{path ? tilde(path) : m.settings_terminal_shell_not_found()}</span
					>
				</div>
			{/each}
		</div>
		{#if info.path.length}
			<button class="toggle" onclick={() => (showPath = !showPath)}>
				<ChevronRight
					size={13}
					style="transform: rotate({showPath ? 90 : 0}deg); transition: transform 150ms"
				/>
				{m.settings_terminal_shell_show_the_full_path_length({ length: info.path.length })}
			</button>
			{#if showPath}<pre class="mono ca-selectable">{info.path.join("\n")}</pre>{/if}
		{/if}
	</Group>
{/if}

<style>
	.select {
		width: 260px;
	}
	.other {
		display: flex;
		gap: 8px;
		align-items: center;
		width: 360px;
	}
	.other :global(input) {
		flex: 1;
		min-width: 0;
	}
	.status {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 10px;
		padding: 11px 16px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-sunken);
		border-radius: 0 0 12px 12px;
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
		padding: 13px 16px 4px;
		font-size: 13px;
		font-weight: 550;
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
