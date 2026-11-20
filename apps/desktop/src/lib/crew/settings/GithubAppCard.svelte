<!--
  A crew server's GitHub App (#643), in Settings > Atlas Crew: create it in one click through GitHub's manifest flow
  (the browser does the form, a one-shot loopback listener brings the code back), then show the app, where it is
  installed, and Manage repositories / Refresh / Remove. The flow lives in `githubApp.svelte.ts`.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { ExternalLink } from "@lucide/svelte";
	import { Button, SegmentedControl, Spinner, Switch, TextField } from "@codeatlasdev/ui";
	import Group from "#lib/settings/Group.svelte";
	import Row from "#lib/settings/Row.svelte";
	import { CrewGithubApp } from "#lib/crew/githubApp.svelte.ts";
	import { clock, permissionLabel, permissionList, validOrgLogin } from "#lib/crew/githubApp.ts";
	import { confirmDialog } from "#lib/stores/ask.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { hostKey, serverName, flow: given }: { hostKey: string; serverName: string; flow?: CrewGithubApp } =
		$props();

	// Made once per card (see BrowsersRow): the flow holds the listener and the watch, and dies with the card.
	// svelte-ignore state_referenced_locally
	const flow = given ?? new CrewGithubApp(hostKey);

	let now = $state(Date.now());
	const left = $derived(flow.secondsLeft(now));
	const orgBad = $derived(flow.owner.kind === "org" && flow.org.trim() !== "" && !validOrgLogin(flow.org.trim()));
	const permissions = $derived(flow.app ? permissionList(flow.app.permissions) : []);

	onMount(() => {
		void flow.load();
		return () => flow.destroy();
	});

	// The countdown ticks only while the browser is being waited for.
	$effect(() => {
		if (flow.phase !== "waiting") return;
		now = Date.now();
		const t = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(t);
	});

	async function remove() {
		const ok = await confirmDialog(m.crew_github_app_remove_title({ server: serverName }), {
			description: m.crew_github_app_remove_description(),
			confirmLabel: m.common_action_remove(),
			destructive: true,
		});
		if (ok) await flow.remove();
	}
</script>

<Group title={m.crew_github_app_group_title({ server: serverName })}>
	{#if flow.phase === "loading"}
		<Row id="crew-github-app-{hostKey}" label={m.crew_github_app_loading()}>
			<Spinner size={14} />
		</Row>
	{:else if flow.app}
		{@const app = flow.app}
		<Row id="crew-github-app-{hostKey}" label={m.crew_github_app_app_name()}>
			<a
				class="app-link"
				href={`https://github.com/apps/${encodeURIComponent(app.slug)}`}
				target="_blank"
				rel="noreferrer"
				>{app.name}<ExternalLink size={11} aria-hidden="true" /></a
			>
		</Row>
		<Row label={m.crew_github_app_app_owner()}><span class="value">{app.owner}</span></Row>
		<Row label={m.crew_github_app_app_identity()}><span class="value mono">{app.identity}</span></Row>
		<Row label={m.crew_github_app_app_permissions()} stack>
			<ul class="chips" aria-label={m.crew_github_app_app_permissions()}>
				{#each permissions as p (p.name)}
					<li class="chip" class:write={p.level === "write"}>
						{permissionLabel(p.name)}
						<span class="level"
							>{p.level === "write"
								? m.crew_github_app_level_write()
								: p.level === "read"
									? m.crew_github_app_level_read()
									: p.level}</span
						>
					</li>
				{/each}
			</ul>
		</Row>
		<Row label={m.crew_github_app_app_installations()} stack>
			{#if app.installations.length > 0}
				<ul class="installs">
					{#each app.installations as i (i.id)}
						<li>
							<span class="value">{i.account}</span>
							<span class="level"
								>{i.repositorySelection === "all"
									? m.crew_github_app_selection_all()
									: m.crew_github_app_selection_selected()}</span
							>
						</li>
					{/each}
				</ul>
			{:else}
				<span class="muted">{m.crew_github_app_not_installed()}</span>
			{/if}
			{#snippet below()}
				{#if app.installationsError}
					<p class="error" role="alert">
						{m.crew_github_app_installations_error({ detail: app.installationsError })}
					</p>
				{/if}
				{#if flow.watching}
					<p class="status" role="status" aria-live="polite">
						<Spinner size={12} />
						{m.crew_github_app_installing()}
					</p>
				{/if}
				{#if flow.error}<p class="error" role="alert">{flow.error}</p>{/if}
				<div class="actions">
					<Button
						size="sm"
						variant={app.installations.length > 0 ? "ghost" : "filled"}
						onclick={() => void flow.openInstall()}
					>
						{app.installations.length > 0
							? m.crew_github_app_manage()
							: m.crew_github_app_install()}
					</Button>
					<Button size="sm" variant="ghost" disabled={flow.refreshing} onclick={() => void flow.refresh()}>
						{flow.refreshing ? m.crew_github_app_refreshing() : m.crew_github_app_refresh()}
					</Button>
					<Button size="sm" variant="ghost" onclick={() => void remove()}>{m.common_action_remove()}</Button>
				</div>
			{/snippet}
		</Row>
	{:else}
		<Row
			id="crew-github-app-{hostKey}"
			label={m.crew_github_app_title()}
			description={m.crew_github_app_intro({ server: serverName })}
			stack
		>
			{#if flow.phase === "waiting"}
				<div class="wait" role="status" aria-live="polite">
					<p class="wait-title"><Spinner size={12} /> {m.crew_github_app_waiting_title()}</p>
					<p class="muted">{m.crew_github_app_waiting_body()}</p>
					<p class="deadline" aria-live="off">{m.crew_github_app_deadline({ time: clock(left) })}</p>
					<p class="muted">{m.crew_github_app_deadline_note()}</p>
					<div class="actions">
						<Button size="sm" variant="ghost" onclick={() => flow.cancel()}>{m.common_action_cancel()}</Button>
					</div>
				</div>
			{:else if flow.phase === "converting"}
				<p class="status" role="status" aria-live="polite">
					<Spinner size={12} />
					{m.crew_github_app_converting()}
				</p>
			{:else}
				<div class="form">
					<div class="field">
						<span class="field-label">{m.crew_github_app_owner_label()}</span>
						<SegmentedControl
							aria-label={m.crew_github_app_owner_label()}
							size="sm"
							value={flow.owner.kind}
							items={[
								{ value: "user", label: m.crew_github_app_owner_user() },
								{ value: "org", label: m.crew_github_app_owner_org() },
							]}
							onchange={(v) => (flow.owner = v === "org" ? { kind: "org" } : { kind: "user" })}
						/>
					</div>
					{#if flow.owner.kind === "org"}
						<TextField
							size="sm"
							label={m.crew_github_app_org_label()}
							placeholder="acme"
							autocomplete="off"
							bind:value={flow.org}
							error={orgBad ? m.crew_github_app_org_invalid() : undefined}
							onkeydown={(e) => e.key === "Enter" && void flow.create()}
						/>
					{/if}
					<div class="workflows">
						<Switch
							label={m.crew_github_app_workflows_label()}
							bind:checked={flow.workflows}
						/>
						<p class="muted">{m.crew_github_app_workflows_reason()}</p>
					</div>
					{#if flow.error}<p class="error" role="alert">{flow.error}</p>{/if}
					<div class="actions">
						<Button
							size="sm"
							disabled={flow.busy || !flow.args}
							onclick={() => void flow.create()}
						>
							{flow.phase === "preparing" ? m.crew_github_app_preparing() : m.crew_github_app_create()}
						</Button>
					</div>
					<p class="muted">{m.crew_github_app_create_hint()}</p>
				</div>
			{/if}
		</Row>
	{/if}
</Group>

<style>
	.form,
	.wait {
		display: flex;
		flex-direction: column;
		gap: 10px;
		width: 100%;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
		align-items: flex-start;
	}
	.field-label {
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-text-secondary);
	}
	.workflows {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	p {
		margin: 0;
	}
	.muted {
		font-size: 12px;
		color: var(--ca-text-secondary);
		line-height: 1.45;
	}
	.wait-title,
	.status {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		font-weight: 600;
		color: var(--ca-text-primary);
	}
	.status {
		font-weight: 400;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.deadline {
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-primary);
	}
	.error {
		font-size: 12px;
		color: var(--ca-semantic-error, var(--ca-text-primary));
	}
	.actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
		margin-top: 4px;
	}
	.value {
		font-size: 12px;
		color: var(--ca-text-primary);
	}
	.mono {
		font-family: var(--ca-font-mono, ui-monospace, monospace);
	}
	.app-link {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	.app-link:hover {
		text-decoration: underline;
	}
	.chips,
	.installs {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.installs {
		flex-direction: column;
		gap: 4px;
	}
	.installs li {
		display: flex;
		gap: 8px;
		align-items: baseline;
	}
	.chip {
		display: inline-flex;
		gap: 5px;
		align-items: baseline;
		padding: 2px 8px;
		border-radius: 999px;
		border: 0.5px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.chip.write {
		color: var(--ca-text-primary);
	}
	.level {
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
</style>
