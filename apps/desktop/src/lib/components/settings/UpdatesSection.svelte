<script lang="ts">
	/** Settings ▸ General ▸ Updates: release status, CLI/app versions, dev link, install progress. */
	import { Alert, Button, Kbd, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import {
		AppWindow,
		ArrowRight,
		CheckCircle2,
		CircleAlert,
		Copy,
		Download,
		ExternalLink,
		FlaskConical,
		RefreshCw,
		RotateCcw,
		Sparkles,
		Terminal,
		Unlink,
	} from "@lucide/svelte";
	import { api } from "#lib/api.ts";
	import { tilde } from "#lib/format.ts";
	import { dateTime } from "#lib/i18n/fmt.ts";
	import { m } from "#lib/i18n/m.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import {
		DEFAULT_CHANNEL,
		channelName,
		managedView,
		stepLabel,
		updateHeadline,
		updateStatus,
	} from "#lib/updates.ts";
	import HostStatusRow from "./HostStatusRow.svelte";
	import SectionTitle from "./SectionTitle.svelte";
	import Tile from "./Tile.svelte";
	import UpdateChannel from "./UpdateChannel.svelte";

	const CHANGELOG_URL = "https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md";
	const LINK_CMD = "atlas link --desktop";
	const DEV_CMD = "atlas desktop --dev";

	let logEl: HTMLPreElement | undefined = $state();
	let notesOpen = $state(false);

	$effect(() => {
		u.log.length;
		if (logEl) logEl.scrollTop = logEl.scrollHeight;
	});

	/** One state at a time: while a check runs there is no verdict ("up to date" would contradict "Checking…"). */
	let status = $derived(
		updateStatus({
			relaunchNeeded: u.relaunchNeeded,
			linked: u.linked,
			installing: u.running === "update",
			checking: u.checking,
			failed: !!u.error,
			available: u.available,
			checked: !!u.release,
		}),
	);
	let headline = $derived(updateHeadline(status, u.release?.version));

	let checked = $derived(
		u.checkedAt
			? m.settings_updates_checked_at({
					time: dateTime(u.checkedAt, { hour: "2-digit", minute: "2-digit" }),
				})
			: "",
	);

	let channelLabel = $derived(channelName(u.channelState?.channel ?? DEFAULT_CHANNEL));

	let managed = $derived(u.outcome === "managed" && u.available ? managedView(u.progress.managed) : null);

	async function copy(text: string) {
		await navigator.clipboard.writeText(text);
		toast.success(m.viewers_files_copy_copied());
	}
</script>

<SectionTitle title={m.settings_registry_general_group_updates()} setting="updates" first />
<div
	class="hero"
	class:avail={status === "available"}
	class:dev={status === "dev"}
	class:quiet={status === "checking" || status === "unknown"}
>
	<div class="badge">
		{#if status === "dev"}<FlaskConical size={20} />{:else if status === "available"}<Sparkles
				size={20}
			/>{:else if status === "checking"}<Spinner size={20} />{:else if status === "unknown"}<CircleAlert
				size={20}
			/>{:else}<CheckCircle2 size={20} />{/if}
	</div>
	<div class="htext">
		<h2>{headline}</h2>
		<p>
			{#if status === "checking"}{m.settings_updates_looking_newest()}{:else if checked}{checked}{/if}
			<span class="chip mono">{u.info?.app.version ?? "—"}</span>
			<span class="chip">{channelLabel}</span>
			{#if u.release}
				<button class="link" onclick={() => api.openUrl(u.release!.url)}>
					{m.settings_updates_release_on_github()}
					<ExternalLink size={11} />
				</button>
			{/if}
		</p>
	</div>
	<div class="hact">
		{#if u.relaunchNeeded}
			<Button onclick={u.relaunch}>
				{#snippet leading()}<RotateCcw size={14} />{/snippet}
				{m.settings_updates_restart_atlas()}
			</Button>
		{:else if status === "dev"}
			<Button variant="outlined" disabled={!!u.running} onclick={u.unlink}>
				{#snippet leading()}{#if u.running === "unlink"}<Spinner size={14} />{:else}<Unlink
							size={14}
						/>{/if}{/snippet}
				{m.settings_updates_unlink_dev()}
			</Button>
		{:else if status === "available" || status === "installing"}
			<Button disabled={!!u.running} onclick={u.updateAll}>
				{#snippet leading()}{#if u.running === "update"}<Spinner size={14} />{:else}<Download
							size={14}
						/>{/if}{/snippet}
				{u.running === "update" ? m.settings_updates_updating() : m.settings_updates_update_all()}
			</Button>
		{/if}
		<Button variant="outlined" disabled={u.checking || !!u.running} onclick={() => u.check()}>
			{#snippet leading()}<RefreshCw size={13} />{/snippet}
			{m.settings_updates_check_now()}
		</Button>
	</div>
	<div class="hfoot">
		<label class="auto" title={m.settings_updates_auto_hint()}>
			<Switch
				checked={u.prefs.auto}
				aria-label={m.settings_updates_auto_aria()}
				onchange={(v) => u.setAuto(v)}
			/>
			{m.settings_updates_auto_label()}
		</label>
		<button class="link" onclick={() => api.openUrl(CHANGELOG_URL)}>
			{m.settings_updates_full_changelog()}
			<ExternalLink size={11} />
		</button>
	</div>
</div>

{#if managed?.command}
	<Alert severity="info" title={managed.title}>
		{m.settings_updates_managed_run()}
		<code class="mono ca-selectable">{managed.command}</code>
		<Button variant="outlined" onclick={() => copy(managed.command!)}>
			{#snippet leading()}<Copy size={13} />{/snippet}
			{m.crew_onboarding_connect_step_copy_command()}
		</Button>
		{#if managed.installable}
			<Button disabled={!!u.running} onclick={u.installPackage}>
				{m.settings_updates_install_now_password()}
			</Button>
		{/if}
	</Alert>
{/if}

{#if u.error}
	<Alert severity="error" title={m.settings_updates_check_failed()}>{u.error}</Alert>
{/if}

{#if u.release?.yanked}
	<Alert severity="warning" title={m.settings_updates_withdrawn_title()}>
		{u.release.yanked.reason}
		{#if u.release.yanked.replacement}{m.settings_updates_withdrawn_replacement({
				replacement: u.release.yanked.replacement,
			})}{/if}
		{#if u.available}{m.settings_updates_withdrawn_update()}{:else if u.aheadOfChannel}{m.settings_updates_withdrawn_older()}{/if}
	</Alert>
{/if}

{#if u.available && !u.pill && !u.linked}
	<p class="skipnote">
		{m.settings_updates_skipped_note({ version: u.release?.version ?? "" })}
		<button class="link" onclick={u.unskip}>{m.settings_updates_stop_skipping()}</button>
	</p>
{/if}

<div class="tiles">
	<Tile
		icon={Terminal}
		name="Atlas CLI"
		sub={u.info?.cli.path ? tilde(u.info.cli.path) : (u.info?.cli.error ?? "…")}
		title={u.info?.cli.path ? tilde(u.info.cli.path) : undefined}
		mono
	>
		{#snippet badges()}{#if u.info?.cli.linked_to}<span class="tag info">
					{m.settings_updates_tag_linked()}
				</span>{/if}{/snippet}
		{#snippet version()}
			{u.info?.cli.version ?? "—"}
			{#if u.cliBehind && !u.info?.cli.linked_to}<ArrowRight size={12} /><b>{u.release?.version}</b>{/if}
		{/snippet}
	</Tile>
	<Tile
		icon={AppWindow}
		name="Atlas Desktop"
		sub={u.info?.app.dev ? "cargo tauri dev" : tilde(u.info?.app.bundle)}
		title={u.info?.app.dev ? undefined : tilde(u.info?.app.bundle)}
		mono
	>
		{#snippet badges()}
			{#if u.info?.app.dev}<span class="tag warn">
					{m.settings_general_nav_dev()}
				</span>{:else if u.info?.app.linked_to}<span class="tag info">
					{m.settings_updates_tag_linked()}
				</span>{/if}
		{/snippet}
		{#snippet version()}
			{u.info?.app.version ?? "—"}
			{#if u.appBehind && !u.linked}<ArrowRight size={12} /><b>{u.release?.version}</b>{/if}
		{/snippet}
	</Tile>
	<HostStatusRow />
</div>

{#if u.aheadOfChannel && !u.linked}
	<p class="skipnote">
		{m.settings_updates_ahead_note({
			channel: channelName(u.channelState?.channel ?? DEFAULT_CHANNEL),
			version: u.release?.version ?? "",
		})}
	</p>
{/if}

<UpdateChannel />

{#if u.linked}
	<div class="devbox">
		<div class="devtitle"><FlaskConical size={14} /> {m.settings_updates_dev_link_title()}</div>
		<p>{m.settings_updates_dev_link_body()}</p>
		<div class="cmds">
			<button class="cmd mono" onclick={() => copy(LINK_CMD)}>
				<Kbd>{m.settings_updates_dev_rebuild()}</Kbd>
				{LINK_CMD}
			</button>
			<button class="cmd mono" onclick={() => copy(DEV_CMD)}>
				<Kbd>{m.settings_updates_dev_hot_reload()}</Kbd>
				{DEV_CMD}
			</button>
		</div>
	</div>
{/if}

{#if u.running || u.log.length}
	<ol class="isteps" aria-live="polite">
		{#each u.steps as s (s.id)}
			<li class={s.state}>
				{#if s.state === "done"}<CheckCircle2 size={13} />{:else if s.state === "active"}<Spinner
						size={12}
					/>{:else if s.state === "failed"}<CircleAlert size={13} />{:else}<i></i>{/if}
				{stepLabel(s.id)}
			</li>
		{/each}
	</ol>
	<button class="link" onclick={() => (notesOpen = !notesOpen)}>
		{notesOpen ? m.settings_updates_details_hide() : m.settings_updates_details_show()}
	</button>
	{#if notesOpen}<pre class="log mono ca-selectable" bind:this={logEl}>{u.log.join("\n") ||
				m.settings_updates_log_starting()}</pre>{/if}
{/if}

<div class="foot">
	{#if u.available && !u.linked && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>{m.shell_commands_whats_new()}</Button>
	{:else if u.aheadOfChannel && u.info && !u.info.app.dev && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>{m.shell_commands_whats_new()}</Button>
		<Button variant="outlined" disabled={!!u.running} onclick={u.downgrade}>
			{#snippet leading()}{#if u.running === "update"}<Spinner size={14} />{:else}<Download
						size={14}
					/>{/if}{/snippet}
			{m.settings_updates_install_downgrade({ version: u.release?.version ?? "" })}
		</Button>
	{:else if u.info && !u.info.app.dev && !u.linked && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>{m.shell_commands_whats_new()}</Button>
		<Button variant="outlined" disabled={!!u.running} onclick={u.reinstall}>
			{#snippet leading()}{#if u.running === "reinstall"}<Spinner size={14} />{:else}<Download
						size={14}
					/>{/if}{/snippet}
			{m.settings_updates_reinstall_app()}
		</Button>
	{/if}
</div>

<style>
	.hero h2 {
		margin: 0;
		font-size: 15px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	.hero p {
		margin: 3px 0 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px 8px;
	}
	/* Two rows: status + actions, then the quiet controls under a hairline. */
	.hero {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px 14px;
		padding: 14px 16px 10px;
		border-radius: 12px;
		background: color-mix(in oklab, var(--tone) 7%, var(--ca-surface-raised));
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--tone) 28%, transparent);
		--tone: var(--ca-semantic-success);
	}
	.hero.avail {
		--tone: var(--ca-brand-main);
	}
	.hero.dev {
		--tone: var(--ca-semantic-warning);
	}
	.hero.quiet {
		--tone: var(--ca-text-secondary);
	}
	.htext {
		flex: 1 1 260px;
		min-width: 0;
	}
	.hact {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
	.hfoot {
		flex: 1 1 100%;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding-top: 8px;
		border-top: 0.5px solid color-mix(in oklab, var(--tone) 22%, transparent);
	}
	.auto {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		flex: none;
		height: 19px;
		font-size: 11px;
		font-weight: 600;
		line-height: 1;
		padding: 0 7px;
		white-space: nowrap;
		border-radius: 999px;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--tone) 12%, transparent);
	}
	.badge {
		width: 38px;
		height: 38px;
		border-radius: 11px;
		display: grid;
		place-items: center;
		color: var(--tone);
		background: color-mix(in oklab, var(--tone) 14%, transparent);
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--tone) 40%, transparent);
	}
	/* Three tiles in a row; they wrap to two, then one, as the dialog narrows. */
	.tiles {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
		gap: 10px;
	}
	.link {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		align-self: flex-start;
		border: 0;
		background: none;
		padding: 0;
		color: var(--ca-brand-main);
		font: inherit;
		font-size: 12px;
	}
	.link:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}
	.tiles :global(.ver b) {
		color: var(--ca-brand-main);
	}
	.devbox {
		border-radius: 10px;
		padding: 12px 14px;
		background: color-mix(in oklab, var(--ca-semantic-warning) 9%, transparent);
		border: 0.5px solid color-mix(in oklab, var(--ca-semantic-warning) 30%, transparent);
		font-size: 12.5px;
	}
	.devtitle {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
		color: var(--ca-semantic-warning);
	}
	.devbox p {
		margin: 6px 0 10px;
		color: var(--ca-text-secondary);
	}
	.cmds {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.cmd {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
		color: var(--ca-text-primary);
		border-radius: 8px;
		padding: 5px 9px;
		font-size: 11.5px;
		cursor: copy;
	}
	.cmd:hover {
		border-color: var(--ca-brand-main);
	}
	.isteps {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 6px 16px;
	}
	.isteps li {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.isteps li.done {
		color: var(--ca-semantic-success);
	}
	.isteps li.active {
		color: var(--ca-text-primary);
	}
	.isteps li.failed {
		color: var(--ca-semantic-error);
	}
	.isteps li i {
		width: 5px;
		height: 5px;
		margin: 0 4px;
		border-radius: 50%;
		background: currentColor;
		color: var(--ca-text-disabled);
	}
	.log {
		max-height: 180px;
		overflow: auto;
		margin: 0;
		padding: 10px 12px;
		border-radius: 10px;
		background: var(--ca-surface-sunken);
		font-size: 11.5px;
		white-space: pre-wrap;
	}
	.foot {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}
	.foot:empty {
		display: none;
	}
	.skipnote {
		margin: 8px 2px 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
</style>
