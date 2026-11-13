<script lang="ts">
	import { Button, Spinner, StatusChip, Switch } from "@codeatlasdev/ui";
	import { Copy } from "@lucide/svelte";
	import type { LspLanguagePref } from "#lib/prefs.ts";
	import type { CatalogEntry, ServerStatus } from "#lib/lsp/types.ts";
	import CustomServer from "./CustomServer.svelte";
	import {
		canUninstall,
		hasCustom,
		isBusy,
		patchLanguage,
		serverView,
		sourceBadge,
		versionOf,
		type ServerView,
	} from "./languages.ts";

	let {
		entry,
		servers,
		lsp,
		pending,
		oninstall,
		onuninstall,
		onrestart,
		onlog,
		oncopy,
		onsave,
		anchor = false,
	}: {
		entry: CatalogEntry;
		/** First row of the list: also the target of the "Custom language server" search result. */
		anchor?: boolean;
		servers: ServerStatus[];
		lsp: Record<string, LspLanguagePref>;
		/** An install, uninstall or restart of this server is in flight. */
		pending: boolean;
		oninstall: () => void;
		onuninstall: () => void;
		onrestart: () => void;
		onlog: () => void;
		oncopy: (text: string) => void;
		onsave: (next: Record<string, LspLanguagePref>) => void;
	} = $props();

	let view = $derived(serverView(entry, servers));
	let badge = $derived(sourceBadge(entry, servers));
	let version = $derived(versionOf(entry, servers));
	let busy = $derived(pending || isBusy(entry, servers));
	let installed = $derived(entry.detected.source !== undefined);
	let running = $derived(servers.some((s) => s.name === entry.name && s.state !== "missing"));
	let blocked = $derived(entry.detected.blocked);
	let tail = $derived(servers.find((s) => s.name === entry.name && s.state === "crashed")?.tail ?? []);
	let customOpen = $state(false);

	const CHIP: Record<ServerView["kind"], "success" | "warning" | "error" | "info" | "neutral"> = {
		downloading: "info",
		starting: "info",
		running: "success",
		crashed: "error",
		path: "success",
		managed: "success",
		blocked: "warning",
		missing: "neutral",
	};

	/** A language is on unless its override says `disabled`. */
	const enabled = (language: string) => lsp[language]?.disabled !== true;
</script>

<div class="server" data-setting={anchor ? "lsp-custom" : undefined}>
	<div class="head">
		<div class="title">
			<span class="name">{entry.label}</span>
			<code class="id">{entry.name}</code>
			{#if version}<span class="ver">{version}</span>{/if}
			{#if badge}<span class="badge">{badge}</span>{/if}
		</div>
		<div class="state">
			{#if view.kind === "downloading" || view.kind === "starting"}<Spinner size={14} />{/if}
			<StatusChip status={CHIP[view.kind]} label={view.kind === "blocked" ? "Needs setup" : view.text} />
		</div>
	</div>

	<div class="langs" role="group" aria-label="Languages this server handles">
		<span class="hint">Enabled for</span>
		{#each entry.languages as language, i (language)}
			<label class="lang">
				<Switch
					aria-label="{entry.languageLabels[i] ?? language} language server enabled"
					checked={enabled(language)}
					onchange={(v) => onsave(patchLanguage(lsp, language, { disabled: v ? undefined : true }))}
				/>
				<span>{entry.languageLabels[i] ?? language}</span>
			</label>
		{/each}
	</div>

	{#if entry.detected.path}
		<div class="path"><span class="k">Path</span><code>{entry.detected.path}</code></div>
	{/if}

	{#if blocked}
		<div class="blocked" role="alert">
			<strong>{blocked.message}</strong>
			{#if blocked.hint}<span>{blocked.hint}</span>{/if}
			{#if blocked.command}
				<span class="cmd">
					<code>{blocked.command}</code>
					<Button
						size="sm"
						variant="ghost"
						aria-label="Copy command"
						onclick={() => oncopy(blocked.command ?? "")}
					>
						<Copy size={12} /> Copy
					</Button>
				</span>
			{/if}
		</div>
	{/if}

	{#if view.kind === "crashed"}
		<div class="blocked" role="alert">
			<strong>{view.text}</strong>
			{#if tail.length > 0}<pre class="tail">{tail.join("\n")}</pre>{/if}
		</div>
	{/if}

	<div class="actions">
		{#if installed}
			{#if canUninstall(entry)}
				<Button size="sm" variant="outlined" disabled={busy} onclick={oninstall}>Update</Button>
				<Button size="sm" variant="outlined" disabled={busy} onclick={onuninstall}>Uninstall</Button>
			{/if}
		{:else}
			<Button size="sm" variant="filled" disabled={busy || !!blocked} loading={busy} onclick={oninstall}
				>Install</Button
			>
		{/if}
		{#if running}
			<Button size="sm" variant="outlined" disabled={busy} onclick={onrestart}>Restart</Button>
		{/if}
		<Button size="sm" variant="ghost" onclick={onlog}>Open log</Button>
		<span class="grow"></span>
		<Button size="sm" variant="ghost" onclick={() => (customOpen = !customOpen)} aria-expanded={customOpen}>
			{customOpen ? "Hide custom server" : "Custom server"}
			{#if !customOpen && entry.languages.some((l) => hasCustom(lsp[l]))}·&nbsp;set{/if}
		</Button>
	</div>

	{#if customOpen}
		<div class="customs">
			{#each entry.languages as language, i (language)}
				<div class="custom-lang">
					{#if entry.languages.length > 1}<h4>{entry.languageLabels[i] ?? language}</h4>{/if}
					<CustomServer
						{language}
						label={entry.languageLabels[i] ?? language}
						pref={lsp[language]}
						onsave={(patch) => onsave(patchLanguage(lsp, language, patch))}
						onreset={() =>
							onsave(
								patchLanguage(lsp, language, {
									command: undefined,
									args: undefined,
									initialization_options: undefined,
									settings: undefined,
								}),
							)}
					/>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.server {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px 16px;
		min-width: 0;
	}
	.head {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 6px 12px;
	}
	.title {
		display: flex;
		align-items: baseline;
		flex-wrap: wrap;
		flex: 1 1 200px;
		gap: 6px 8px;
		min-width: 0;
	}
	.name {
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.id,
	.path code,
	.cmd code {
		font-family: ui-monospace, monospace;
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.ver {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.badge {
		display: inline-flex;
		align-items: center;
		flex: none;
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 10.5px;
		font-weight: 550;
		line-height: 1.3;
		white-space: nowrap;
		color: var(--ca-text-secondary);
		background: var(--ca-surface-sunken);
		border: 0.5px solid var(--ca-outline-subtle);
	}
	/* The status keeps its full text (it is what the row is for); when the row is too narrow for name and status side by
	   side, the status drops under the name instead of being squeezed to nothing. */
	.state {
		display: flex;
		align-items: center;
		gap: 6px;
		flex: none;
		max-width: 100%;
	}
	.langs {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px 14px;
	}
	.lang {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-primary);
	}
	.hint {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.path {
		display: flex;
		gap: 8px;
		align-items: baseline;
		min-width: 0;
	}
	.path code {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		user-select: text;
		-webkit-user-select: text;
	}
	.k {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.blocked {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 8px 10px;
		border-radius: 8px;
		font-size: 12px;
		line-height: 1.4;
		color: var(--ca-text-secondary);
		background: color-mix(in oklab, var(--ca-semantic-warning) 10%, transparent);
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 35%, transparent);
	}
	.blocked strong {
		color: var(--ca-text-primary);
		font-weight: 600;
	}
	.cmd {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.cmd code {
		user-select: text;
		-webkit-user-select: text;
	}
	.tail {
		margin: 0;
		max-height: 120px;
		overflow: auto;
		font-family: ui-monospace, monospace;
		font-size: 11px;
		white-space: pre-wrap;
		user-select: text;
		-webkit-user-select: text;
	}
	/* Primary action first, secondary after it, the quiet "Custom server" toggle at the far end. */
	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
		margin-top: 2px;
		padding-top: 10px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.grow {
		flex: 1;
	}
	.customs {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding-top: 4px;
	}
	.custom-lang h4 {
		margin: 0 0 8px;
		font-size: 12px;
		font-weight: 650;
		color: var(--ca-text-secondary);
	}
</style>
