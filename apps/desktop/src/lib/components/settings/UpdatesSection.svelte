<script lang="ts">
	/** Settings ▸ General ▸ Updates: release status, CLI/app versions, dev link, install progress. */
	import { Alert, Button, Kbd, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import {
		AppWindow,
		ArrowRight,
		CheckCircle2,
		CircleAlert,
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
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import Group from "./Group.svelte";
	import HostStatusRow from "./HostStatusRow.svelte";
	import Row from "./Row.svelte";
	import SectionTitle from "./SectionTitle.svelte";
	import UpdateChannel from "./UpdateChannel.svelte";

	const CHANGELOG_URL = "https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md";

	let logEl: HTMLPreElement | undefined = $state();
	let notesOpen = $state(false);

	$effect(() => {
		u.log.length;
		if (logEl) logEl.scrollTop = logEl.scrollHeight;
	});

	let headline = $derived(
		u.relaunchNeeded
			? "Restart to finish updating"
			: u.linked
				? "Running a dev build"
				: u.available
					? `Atlas ${u.release?.version} is available`
					: u.release
						? "Atlas is up to date"
						: "Atlas versions",
	);

	let checked = $derived(
		u.checkedAt
			? `Checked at ${new Date(u.checkedAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
			: "",
	);

	async function copy(text: string) {
		await navigator.clipboard.writeText(text);
		toast.success("Copied");
	}
</script>

<SectionTitle title="Updates" setting="updates" first />
<div class="hero" class:avail={u.available && !u.linked} class:dev={u.linked}>
	<div class="badge">
		{#if u.linked}<FlaskConical size={22} />{:else if u.available}<Sparkles size={22} />{:else}<CheckCircle2
				size={22}
			/>{/if}
	</div>
	<div class="htext">
		<h2>{headline}</h2>
		<p>
			{#if u.checking}<Spinner size={11} /> Checking…{:else}{checked}{/if}
			{#if u.release}
				<button class="link" onclick={() => api.openUrl(u.release!.url)}
					>Release on GitHub <ExternalLink size={11} /></button
				>
			{/if}
		</p>
	</div>
	<div class="hact">
		{#if u.relaunchNeeded}
			<Button onclick={u.relaunch}>
				{#snippet leading()}<RotateCcw size={14} />{/snippet}
				Restart Atlas
			</Button>
		{:else if u.linked}
			<Button variant="outlined" disabled={!!u.running} onclick={u.unlink}>
				{#snippet leading()}{#if u.running === "unlink"}<Spinner size={14} />{:else}<Unlink
							size={14}
						/>{/if}{/snippet}
				Unlink dev build
			</Button>
		{:else if u.available}
			<Button disabled={!!u.running} onclick={u.updateAll}>
				{#snippet leading()}{#if u.running === "update"}<Spinner size={14} />{:else}<Download
							size={14}
						/>{/if}{/snippet}
				{u.running === "update" ? "Updating…" : "Update CLI and app"}
			</Button>
		{/if}
		<Button variant="outlined" disabled={u.checking || !!u.running} onclick={() => u.check()}>
			{#snippet leading()}<RefreshCw size={13} />{/snippet}
			Check now
		</Button>
	</div>
</div>

{#if u.error}
	<Alert severity="error" title="Could not check for updates">{u.error}</Alert>
{/if}

{#if u.available && !u.pill && !u.linked}
	<p class="skipnote">
		You skipped {u.release?.version}, so Atlas will not remind you. It will tell you about the next one.
		<button class="link" onclick={u.unskip}>Stop skipping</button>
	</p>
{/if}

<Group>
	<Row
		icon={Terminal}
		name="Atlas CLI"
		sub={u.info?.cli.path ? tilde(u.info.cli.path) : (u.info?.cli.error ?? "…")}
		mono
	>
		{#if u.info?.cli.linked_to}<span class="tag info">linked</span>{/if}
		<div class="ver mono">
			{u.info?.cli.version ?? "—"}
			{#if u.cliBehind && !u.info?.cli.linked_to}<ArrowRight size={12} /><b>{u.release?.version}</b>{/if}
		</div>
	</Row>
	<Row
		icon={AppWindow}
		name="Atlas Desktop"
		sub={u.info?.app.dev ? "cargo tauri dev" : tilde(u.info?.app.bundle)}
		mono
	>
		{#if u.info?.app.dev}<span class="tag warn">Dev</span>{:else if u.info?.app.linked_to}<span
				class="tag info">linked</span
			>{/if}
		<div class="ver mono">
			{u.info?.app.version ?? "—"}
			{#if u.appBehind && !u.linked}<ArrowRight size={12} /><b>{u.release?.version}</b>{/if}
		</div>
	</Row>
	<HostStatusRow />
	<Row icon={Sparkles} name="Changelog" sub="Every release, newest first">
		<button class="link" onclick={() => api.openUrl(CHANGELOG_URL)}
			>Full changelog <ExternalLink size={11} /></button
		>
	</Row>
	<Row
		icon={RefreshCw}
		name="Check automatically"
		sub="On launch, when you return to the app, and every 6 hours"
	>
		<Switch
			checked={u.prefs.auto}
			aria-label="Check for updates automatically"
			onchange={(v) => u.setAuto(v)}
		/>
	</Row>
</Group>

{#if u.aheadOfChannel && !u.linked}
	<p class="skipnote">
		You run a newer build than the {u.channelState?.channel ?? "selected"} channel offers ({u.release
			?.version}), so no update is offered. Pick a more adventurous channel to keep moving forward, or install
		{u.release?.version} explicitly with the button at the bottom (settings written by the newer build may not be
		readable).
	</p>
{/if}

<UpdateChannel />

{#if u.linked}
	<div class="devbox">
		<div class="devtitle"><FlaskConical size={14} /> Dev link active</div>
		<p>This build runs from your local checkout, so Atlas will not replace it with a release.</p>
		<div class="cmds">
			<button class="cmd mono" onclick={() => copy("atlas link --desktop")}
				><Kbd>rebuild</Kbd> atlas link --desktop</button
			>
			<button class="cmd mono" onclick={() => copy("atlas desktop --dev")}
				><Kbd>hot reload</Kbd> atlas desktop --dev</button
			>
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
				{s.label}
			</li>
		{/each}
	</ol>
	<button class="link" onclick={() => (notesOpen = !notesOpen)}>{notesOpen ? "Hide" : "Show"} details</button>
	{#if notesOpen}<pre class="log mono ca-selectable" bind:this={logEl}>{u.log.join("\n") ||
				"starting…"}</pre>{/if}
{/if}

<div class="foot">
	{#if u.available && !u.linked && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>What's new</Button>
	{:else if u.aheadOfChannel && u.info && !u.info.app.dev && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>What's new</Button>
		<Button variant="outlined" disabled={!!u.running} onclick={u.downgrade}>
			{#snippet leading()}{#if u.running === "update"}<Spinner size={14} />{:else}<Download
						size={14}
					/>{/if}{/snippet}
			Install {u.release?.version} now (downgrade)
		</Button>
	{:else if u.info && !u.info.app.dev && !u.linked && !u.relaunchNeeded}
		<Button variant="ghost" onclick={u.showWhatsNew}>What's new</Button>
		<Button variant="outlined" disabled={!!u.running} onclick={u.reinstall}>
			{#snippet leading()}{#if u.running === "reinstall"}<Spinner size={14} />{:else}<Download
						size={14}
					/>{/if}{/snippet}
			Reinstall app
		</Button>
	{/if}
</div>

<style>
	.hero h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 650;
		letter-spacing: -0.01em;
	}
	.hero p {
		margin: 3px 0 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.hero {
		display: flex;
		align-items: center;
		gap: 16px;
		padding: 16px 18px;
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
	.htext {
		flex: 1;
		min-width: 0;
	}
	.hact {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
		justify-content: flex-end;
	}
	.badge {
		width: 44px;
		height: 44px;
		border-radius: 12px;
		display: grid;
		place-items: center;
		color: var(--tone);
		background: color-mix(in oklab, var(--tone) 14%, transparent);
		box-shadow: inset 0 0 0 0.5px color-mix(in oklab, var(--tone) 40%, transparent);
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
	.tag {
		font-size: 10.5px;
		font-weight: 600;
		padding: 2px 7px;
		border-radius: 999px;
	}
	.tag.info {
		color: var(--ca-semantic-info);
		background: color-mix(in oklab, var(--ca-semantic-info) 14%, transparent);
	}
	.tag.warn {
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 14%, transparent);
	}
	.ver {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.ver b {
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
