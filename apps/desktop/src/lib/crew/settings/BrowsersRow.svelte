<!-- A crew server's capture browsers: what is missing, and the Install button that runs `atlas crew browsers install`. -->
<script lang="ts">
	import { Button, Spinner } from "@codeatlasdev/ui";
	import Row from "#lib/settings/Row.svelte";
	import { BROWSERS_COST, BROWSERS_WHY, nodeProblem } from "#lib/crew/browsers.ts";
	import { CrewBrowsers } from "#lib/crew/browsers.svelte.ts";
	import { crew } from "#lib/stores/crew.svelte.ts";

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
	label="Screenshots for UI work"
	description="{BROWSERS_WHY} {BROWSERS_COST}"
	stack
>
	<div class="browsers" data-testid="crew-browsers">
		<div class="line">
			<input
				class="input"
				type="text"
				placeholder="ubuntu@my-server.com"
				aria-label="Admin SSH address"
				bind:value={cb.address}
				disabled={cb.busy}
				onblur={() => void cb.saveAddress()}
				onkeydown={(e) => e.key === "Enter" && void cb.check()}
			/>
			<Button size="sm" variant="ghost" disabled={!cb.canCheck} onclick={() => void cb.check()}>
				{view?.phase === "checking" ? "Checking…" : "Check"}
			</Button>
		</div>
		<p class="hint">
			The address you installed with, not the crew user: installing the browsers needs <code>sudo</code> on the
			server. Atlas keeps this address, nothing else.
		</p>
		{#if cb.saveError}
			<p class="error" role="alert">Could not save the address: {cb.saveError}</p>
		{/if}

		{#if view?.phase === "checking"}
			<span class="state"><Spinner size={12} /> Asking the server…</span>
		{:else if view?.phase === "ready"}
			<span class="state ok" role="status">Installed: Chromium and WebKit are ready.</span>
		{:else if view?.phase === "missing"}
			{#if nodeLine}
				<p class="node" role="alert">
					<b>Node.js 20 or newer is required.</b>
					{nodeLine}. Install it on the server, then check again.
				</p>
			{/if}
			<ul class="problems" aria-label="Missing pieces">
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
				placeholder="Sudo password"
				aria-label="Sudo password"
				bind:value={cb.password}
				disabled={cb.busy}
			/>
		{/if}

		{#if view?.phase === "missing" || cb.installing || cb.installError}
			<div class="line">
				<Button size="sm" variant="outlined" disabled={!cb.canInstall} onclick={() => void cb.install()}>
					{cb.installing ? "Installing…" : "Install"}
				</Button>
			</div>
		{/if}

		{#if cb.installError}
			<p class="error" role="alert">{cb.installError}</p>
		{/if}
		{#if cb.log.length > 0}
			<pre class="log" aria-label="Install log">{cb.log.join("\n")}</pre>
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
