<!-- A crew server's capture browsers: what is missing, and the Install button that runs `atlas crew browsers install`. -->
<script lang="ts">
	import { Button, Spinner } from "@codeatlasdev/ui";
	import Row from "#lib/settings/Row.svelte";
	import { nodeProblem } from "#lib/crew/browsers.ts";
	import { CrewBrowsers } from "#lib/crew/browsers.svelte.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";
	import { m } from "#lib/i18n/m.ts";

	let { hostKey, cb: given }: { hostKey: string; cb?: CrewBrowsers } = $props();

	// Made once per row, never from a prop default: that default is a derivation, and the constructor reads
	// `crew.servers` (the registry's address), so saving the address would build a fresh state and drop the result.
	// The address starts from the registry's and edits are saved back to it.
	// svelte-ignore state_referenced_locally
	const cb =
		given ??
		new CrewBrowsers({
			current: () => crew.servers.find((h) => h.key === hostKey)?.admin,
			save: (admin) => crew.setAdmin(hostKey, admin),
		});

	const view = $derived(cb.view);
	const nodeLine = $derived(view?.phase === "missing" && view.needsNode ? nodeProblem(view.problems) : null);
</script>

<Row
	id="crew-browsers-{hostKey}"
	label={m.crew_settings_browsers_row_screenshots_for_ui_work()}
	description={`${m.crew_settings_browsers_row_why()} ${m.crew_settings_browsers_row_cost()}`}
	stack
>
	<div class="browsers" data-testid="crew-browsers">
		<div class="line">
			<input
				class="input"
				type="text"
				placeholder="ubuntu@my-server.com"
				aria-label={m.crew_settings_browsers_row_admin_ssh_address()}
				bind:value={cb.address}
				disabled={cb.busy}
				onblur={() => void cb.saveAddress()}
				onkeydown={(e) => e.key === "Enter" && void cb.check()}
			/>
			<Button size="sm" variant="ghost" disabled={!cb.canCheck} onclick={() => void cb.check()}>
				{view?.phase === "checking"
					? m.crew_settings_browsers_row_checking()
					: m.crew_settings_browsers_row_check()}
			</Button>
		</div>
		<p class="hint">
			{m.crew_settings_browsers_row_address_hint()}
		</p>
		{#if cb.saveError}
			<p class="error" role="alert">
				{m.crew_settings_browsers_row_could_not_save_the_address({ saveError: cb.saveError })}
			</p>
		{/if}

		{#if view?.phase === "checking"}
			<span class="state"><Spinner size={12} /> {m.crew_settings_browsers_row_asking_the_server()}</span>
		{:else if view?.phase === "ready"}
			<span class="state ok" role="status"
				>{m.crew_settings_browsers_row_installed_chromium_and_webkit_are()}</span
			>
		{:else if view?.phase === "missing"}
			{#if nodeLine}
				<p class="node" role="alert">
					<b>{m.crew_settings_browsers_row_node_required()}</b>
					{m.crew_settings_browsers_row_node_install({ problem: nodeLine })}
				</p>
			{/if}
			<ul class="problems" aria-label={m.crew_settings_browsers_row_missing_pieces()}>
				{#each view.problems.filter((p) => p.kind !== "node" && p.kind !== "npm") as p (p.text)}
					<li>{p.text}</li>
				{/each}
			</ul>
		{:else if view?.phase === "error"}
			<p class="error" role="alert">{view.error}</p>
		{/if}

		{#if cb.needsPassword}
			<input
				class="input"
				type="password"
				placeholder={m.crew_settings_browsers_row_sudo_password()}
				aria-label={m.crew_settings_browsers_row_sudo_password()}
				bind:value={cb.password}
				disabled={cb.busy}
			/>
		{/if}

		{#if view?.phase === "missing" || cb.installing || cb.installError}
			<div class="line">
				<Button size="sm" variant="outlined" disabled={!cb.canInstall} onclick={() => void cb.install()}>
					{cb.installing ? m.crew_settings_browsers_row_installing() : m.crew_settings_browsers_row_install()}
				</Button>
			</div>
		{/if}

		{#if cb.installError}
			<p class="error" role="alert">{cb.installError}</p>
		{/if}
		{#if cb.log.length > 0}
			<pre class="log" aria-label={m.crew_settings_browsers_row_install_log()}>{cb.log.join("\n")}</pre>
		{/if}
	</div>
</Row>

<style>
	.browsers {
		display: flex;
		flex-direction: column;
		gap: 8px;
		width: 100%;
	}
	.line {
		display: flex;
		gap: 8px;
		align-items: center;
	}
	.input {
		flex: 1;
		min-width: 160px;
		height: 28px;
		padding: 0 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-base, transparent);
		color: var(--ca-text-primary);
		font-size: 12px;
		outline: none;
	}
	.input:focus {
		border-color: var(--ca-brand-main);
	}
	.hint,
	.state,
	.problems {
		font-size: 12px;
		color: var(--ca-text-secondary);
		margin: 0;
		line-height: 1.5;
	}
	.hint code {
		font-family: monospace;
	}
	.state {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.state.ok {
		color: var(--ca-semantic-success);
	}
	.node {
		font-size: 12.5px;
		margin: 0;
		line-height: 1.5;
		color: var(--ca-text-primary);
	}
	.problems {
		padding-left: 16px;
	}
	.error {
		font-size: 12px;
		margin: 0;
		color: var(--ca-semantic-error, #ef4444);
	}
	.log {
		margin: 0;
		max-height: 180px;
		overflow: auto;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--ca-surface-sunken);
		color: var(--ca-text-secondary);
		font-size: 11.5px;
		line-height: 1.45;
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
