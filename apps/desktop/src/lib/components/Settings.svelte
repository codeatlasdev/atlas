<script lang="ts">
	import { tilde } from "#lib/format.ts";
	import { Alert, Button, Dialog, Kbd, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import {
		ArrowRight,
		CheckCircle2,
		CircleAlert,
		Download,
		ExternalLink,
		FlaskConical,
		RefreshCw,
		RotateCcw,
		Terminal,
		AppWindow,
		Unlink,
		Sparkles,
		Palette,
		Keyboard,
		FolderTree,
		Bot,
		Server,
		CircleHelp,
		Search,
		SlidersHorizontal,
		X,
	} from "@lucide/svelte";
	import { api } from "#lib/api.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import {
		SECTIONS,
		crumb,
		searchSettings,
		type SettingEntry,
		type SettingsSection,
	} from "#lib/settings/index.ts";
	import PageHead from "#lib/settings/PageHead.svelte";
	import ShellSettings from "./ShellSettings.svelte";
	import WorkspaceSettings from "./WorkspaceSettings.svelte";
	import AgentSettings from "./AgentSettings.svelte";
	import SshSettings from "./SshSettings.svelte";
	import EditorSettings from "./EditorSettings.svelte";
	import ShortcutSettings from "./ShortcutSettings.svelte";
	import HelpSettings from "#lib/onboarding/HelpSettings.svelte";
	import { tick } from "svelte";

	const ICONS: Record<SettingsSection, typeof Download> = {
		general: SlidersHorizontal,
		appearance: Palette,
		workspaces: FolderTree,
		agents: Bot,
		keyboard: Keyboard,
		ssh: Server,
		help: CircleHelp,
	};

	let results = $derived(searchSettings(u.query));
	let searching = $derived(u.query.trim() !== "");
	let paneEl: HTMLDivElement | undefined = $state();

	/** Jump to the section/tab of a result and flash the matching row once it renders. */
	async function go(e: SettingEntry) {
		u.section = e.section;
		if (e.tab) u.tab = e.tab;
		u.query = "";
		await tick();
		await new Promise((r) => requestAnimationFrame(r));
		const row = paneEl?.querySelector<HTMLElement>(`[data-setting="${e.id}"]`);
		if (!row) return;
		row.scrollIntoView({ block: "center", behavior: "smooth" });
		row.classList.add("flash");
		setTimeout(() => row.classList.remove("flash"), 1500);
	}
	let logEl: HTMLPreElement | undefined = $state();
	let notesOpen = $state(false);
	const CHANGELOG_URL = "https://github.com/codeatlasdev/atlas/blob/main/CHANGELOG.md";

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

<Dialog bind:open={u.open} size="xl" class="settings-dialog">
	<div class="wrap">
		<nav>
			<div class="navtitle">Settings</div>
			<label class="search">
				<Search size={12} />
				<input placeholder="Search settings" aria-label="Search settings" bind:value={u.query} />
				{#if searching}
					<button class="clear" aria-label="Clear search" onclick={() => (u.query = "")}
						><X size={12} /></button
					>
				{/if}
			</label>
			{#each SECTIONS as n (n.id)}
				{@const Icon = ICONS[n.id]}
				<button
					class="nav"
					class:on={!searching && u.section === n.id}
					onclick={() => {
						u.query = "";
						u.section = n.id;
					}}
				>
					<Icon size={15} />
					{n.label}
					{#if n.id === "general" && u.available && !u.linked}<span
							class="badge-dot"
							aria-label="Update available"
						></span>{/if}
				</button>
			{/each}
			<div class="navfoot mono">
				Atlas {u.info?.app.version ?? ""}{#if u.dev}<span class="devchip">Dev</span>{/if}
			</div>
		</nav>

		<div class="pane" bind:this={paneEl}>
			{#if searching}
				<PageHead
					title={results.length
						? `${results.length} ${results.length === 1 ? "setting" : "settings"}`
						: "No settings found"}
					description={results.length
						? `Matching “${u.query.trim()}”.`
						: "Try a shorter word, like “font”, “theme” or “branch”."}
				/>
				<div class="results">
					{#each results as r (r.section + (r.tab ?? "") + r.id)}
						<button class="result" onclick={() => go(r)}>
							<span class="rtext">
								<span class="rlabel">{r.label}</span>
								<span class="rdesc">{r.description}</span>
							</span>
							<span class="rcrumb">{crumb(r)}</span>
						</button>
					{/each}
				</div>
			{:else if u.section === "general"}
				<PageHead
					title="General"
					description="Keep Atlas up to date and make sure your projects start with the same tools as your terminal."
				/>
				<div class="gtitle first" data-setting="updates"><h3>Updates</h3></div>
				<div class="hero" class:avail={u.available && !u.linked} class:dev={u.linked}>
					<div class="badge">
						{#if u.linked}<FlaskConical size={22} />{:else if u.available}<Sparkles
								size={22}
							/>{:else}<CheckCircle2 size={22} />{/if}
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
					<Button variant="ghost" size="sm" disabled={u.checking || !!u.running} onclick={() => u.check()}>
						{#snippet leading()}<RefreshCw size={13} />{/snippet}
						Check now
					</Button>
				</div>

				{#if u.error}
					<Alert severity="error" title="Could not check for updates">{u.error}</Alert>
				{/if}

				<div class="group">
					<div class="row">
						<div class="ico"><Terminal size={15} /></div>
						<div class="what">
							<div class="name">Atlas CLI</div>
							<div class="sub mono">
								{u.info?.cli.path ? tilde(u.info.cli.path) : (u.info?.cli.error ?? "…")}
							</div>
						</div>
						{#if u.info?.cli.linked_to}<span class="tag info">linked</span>{/if}
						<div class="ver mono">
							{u.info?.cli.version ?? "—"}
							{#if u.cliBehind && !u.info?.cli.linked_to}<ArrowRight size={12} /><b>{u.release?.version}</b
								>{/if}
						</div>
					</div>
					<div class="row">
						<div class="ico"><AppWindow size={15} /></div>
						<div class="what">
							<div class="name">Atlas Desktop</div>
							<div class="sub mono">{u.info?.app.dev ? "cargo tauri dev" : tilde(u.info?.app.bundle)}</div>
						</div>
						{#if u.info?.app.dev}<span class="tag warn">Dev</span>{:else if u.info?.app.linked_to}<span
								class="tag info">linked</span
							>{/if}
						<div class="ver mono">
							{u.info?.app.version ?? "—"}
							{#if u.appBehind && !u.linked}<ArrowRight size={12} /><b>{u.release?.version}</b>{/if}
						</div>
					</div>
					<div class="row">
						<div class="ico"><Sparkles size={15} /></div>
						<div class="what">
							<div class="name">Channel</div>
							<div class="sub">Stable releases from GitHub</div>
						</div>
						<button class="link" onclick={() => api.openUrl(CHANGELOG_URL)}
							>Full changelog <ExternalLink size={11} /></button
						>
					</div>
					<div class="row">
						<div class="ico"><RefreshCw size={15} /></div>
						<div class="what">
							<div class="name">Check automatically</div>
							<div class="sub">On launch, when you return to the app, and every 6 hours</div>
						</div>
						<Switch
							checked={u.prefs.auto}
							aria-label="Check for updates automatically"
							onchange={(v) => u.setAuto(v)}
						/>
					</div>
				</div>

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
					<button class="link" onclick={() => (notesOpen = !notesOpen)}
						>{notesOpen ? "Hide" : "Show"} details</button
					>
					{#if notesOpen}<pre class="log mono selectable" bind:this={logEl}>{u.log.join("\n") ||
								"starting…"}</pre>{/if}
				{/if}

				<div class="foot">
					{#if u.relaunchNeeded}
						<Button onclick={u.relaunch}>
							{#snippet leading()}<RotateCcw size={14} />{/snippet}
							Restart Atlas
						</Button>
					{:else if u.linked}
						<Button variant="glass" disabled={!!u.running} onclick={u.unlink}>
							{#snippet leading()}{#if u.running === "unlink"}<Spinner size={14} />{:else}<Unlink
										size={14}
									/>{/if}{/snippet}
							Unlink dev build
						</Button>
					{:else if u.available}
						<Button variant="glass" onclick={u.showWhatsNew}>What's new</Button>
						<Button disabled={!!u.running} onclick={u.updateAll}>
							{#snippet leading()}{#if u.running === "update"}<Spinner size={14} />{:else}<Download
										size={14}
									/>{/if}{/snippet}
							{u.running === "update" ? "Updating…" : "Update CLI and app"}
						</Button>
					{:else if u.info && !u.info.app.dev}
						<Button variant="ghost" onclick={u.showWhatsNew}>What's new</Button>
						<Button variant="glass" disabled={!!u.running} onclick={u.reinstall}>
							{#snippet leading()}{#if u.running === "reinstall"}<Spinner size={14} />{:else}<Download
										size={14}
									/>{/if}{/snippet}
							Reinstall app
						</Button>
					{/if}
				</div>

				<div class="gtitle" data-setting="shell">
					<h3>Shell for your projects</h3>
					<p>
						Services start with the PATH and variables of this shell, the same as in your terminal. Pick
						another shell if tools like bun or node are not found.
					</p>
				</div>
				<ShellSettings />
			{:else if u.section === "appearance"}
				<EditorSettings />
			{:else if u.section === "workspaces"}
				<WorkspaceSettings />
			{:else if u.section === "agents"}
				<AgentSettings />
			{:else if u.section === "keyboard"}
				<PageHead
					title="Keyboard shortcuts"
					description="Click a shortcut to change it. Every command is listed, whether it has a key or not."
				/>
				<ShortcutSettings />
			{:else if u.section === "ssh"}
				<SshSettings />
			{:else if u.section === "help"}
				<PageHead
					title="Help & onboarding"
					description="Take the tour again, look up a shortcut, or bring the checklist back."
				/>
				<HelpSettings />
			{/if}
		</div>
	</div>
</Dialog>

<style>
	/* Two panes fill the dialog edge to edge. */
	:global(.settings-dialog .ca-dialog__header) {
		display: none;
	}
	:global(.settings-dialog .ca-dialog__body) {
		padding: 0 !important;
	}
	.wrap {
		display: grid;
		grid-template-columns: 210px minmax(0, 1fr);
		height: min(640px, 80vh);
	}
	nav {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 18px 10px 12px;
		background: var(--ca-surface-sunken);
		border-right: 0.5px solid var(--ca-outline-subtle);
	}
	.navtitle {
		font-size: 15px;
		font-weight: 700;
		padding: 0 8px 12px;
	}
	.nav {
		display: flex;
		align-items: center;
		gap: 9px;
		height: 30px;
		padding: 0 8px;
		border: 0;
		border-radius: 7px;
		background: none;
		color: var(--ca-text-secondary);
		font: inherit;
		font-size: 13px;
		font-weight: 500;
		text-align: left;
		transition:
			background 120ms ease-out,
			color 120ms ease-out;
	}
	.nav:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.nav.on {
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 9%, transparent);
	}
	.nav.on :global(svg) {
		color: var(--ca-brand-main);
	}
	.badge-dot {
		margin-left: auto;
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ca-brand-main);
	}
	.navfoot {
		margin-top: auto;
		padding: 0 8px;
		font-size: 11px;
		color: var(--ca-text-disabled);
	}
	.pane {
		min-height: 0;
		overflow-y: auto;
		padding: 22px 24px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	/* A scrolling column: children keep their content height (alerts and groups were squeezed). */
	.pane > :global(*) {
		flex-shrink: 0;
	}
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
		gap: 14px;
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
	.group {
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 14px;
	}
	.row + .row {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.ico {
		width: 30px;
		height: 30px;
		border-radius: 8px;
		display: grid;
		place-items: center;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
	}
	.what {
		flex: 1;
		min-width: 0;
	}
	.name {
		font-weight: 600;
		font-size: 13px;
	}
	.sub {
		font-size: 11px;
		color: var(--ca-text-secondary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
	.devchip {
		margin-left: 6px;
		padding: 1px 6px;
		border-radius: 999px;
		font-family: inherit;
		font-size: 9.5px;
		font-weight: 650;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ca-semantic-warning);
		background: color-mix(in oklab, var(--ca-semantic-warning) 16%, transparent);
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
		opacity: 0.4;
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
	.gtitle {
		margin-top: 10px;
		padding-top: 16px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.gtitle.first {
		margin-top: 0;
		padding-top: 0;
		border-top: 0;
	}
	.gtitle h3 {
		margin: 0;
		font-size: 14px;
		font-weight: 650;
	}
	.gtitle p {
		margin: 3px 0 0;
		font-size: 12.5px;
		line-height: 1.45;
		color: var(--ca-text-secondary);
	}
	.search {
		display: flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		margin: 0 0 10px;
		padding: 0 8px;
		border-radius: 7px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 5%, transparent);
		color: var(--ca-text-secondary);
	}
	.search:focus-within {
		border-color: color-mix(in oklab, var(--ca-brand-main) 55%, var(--ca-outline-subtle));
	}
	.search input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12.5px;
		border: 0;
		outline: none;
		background: transparent;
		color: var(--ca-text-primary);
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.clear {
		display: inline-grid;
		place-items: center;
		border: 0;
		padding: 0;
		background: none;
		color: var(--ca-text-disabled);
	}
	.clear:hover {
		color: var(--ca-text-primary);
	}
	.results {
		display: flex;
		flex-direction: column;
		border-radius: 10px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised);
	}
	.result {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		padding: 10px 14px;
		border: 0;
		background: none;
		font: inherit;
		text-align: left;
		color: inherit;
	}
	.result + .result {
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.result:hover {
		background: var(--ca-action-hover);
	}
	.rtext {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.rlabel {
		font-size: 13px;
		font-weight: 500;
	}
	.rdesc {
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.rcrumb {
		flex-shrink: 0;
		font-size: 11.5px;
		color: var(--ca-text-hint);
	}
</style>
