<script lang="ts">
	import { Button, Drawer, Spinner } from "@codeatlasdev/ui";
	import { Check, ChevronRight, CircleAlert, ExternalLink, RotateCcw, Sparkles } from "@lucide/svelte";
	import { api } from "#lib/api.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import { renderMarkdown } from "#lib/viewers/markdown.ts";
	import { dateOf, stripVersionHeading } from "#lib/updates.ts";

	let detailsOpen = $state(false);
	let logEl: HTMLPreElement | undefined = $state();

	/** The install is under way or has just finished: show steps instead of the notes' call to action. */
	let installing = $derived(u.running === "update" || u.outcome === "done" || u.outcome === "failed");
	let busy = $derived(u.running === "update");
	/** Nothing to install: the sheet shows notes of the running (or just installed) version. */
	let installed = $derived(!u.available || !!u.updatedTo);
	let latest = $derived(u.notes[0]);
	let rendered = $derived(
		u.notes.map((n) => ({ ...n, html: renderMarkdown(stripVersionHeading(n.notes), "") })),
	);

	$effect(() => {
		u.log.length;
		if (logEl) logEl.scrollTop = logEl.scrollHeight;
	});

	function close() {
		u.whatsNewOpen = false;
		u.dismissUpdated();
	}

	/** Release notes are our own CHANGELOG, but links must open in the browser, not navigate the app. */
	function onclick(e: MouseEvent) {
		const a = (e.target as HTMLElement).closest("a");
		if (!a?.href) return;
		e.preventDefault();
		void api.openUrl(a.href);
	}
</script>

<Drawer
	bind:open={u.whatsNewOpen}
	side="right"
	width="480px"
	title="What's new"
	onOpenChange={(o) => !o && u.dismissUpdated()}
>
	<div class="sheet">
		<header class="hero" class:ok={installed}>
			<div class="badge"><Sparkles size={20} /></div>
			<div class="htext">
				<h2>Atlas {latest?.version ?? (installed ? u.info?.app.version : u.release?.version) ?? ""}</h2>
				<p>
					{#if latest}{dateOf(latest.published_at)}{/if}
					{#if installed}<span class="tag">You're up to date</span>
					{:else if u.release && u.info}
						<span class="from mono">{u.info.app.version}</span>
						<ChevronRight size={11} />
						<span class="to mono">{u.release.version}</span>
					{/if}
				</p>
			</div>
		</header>

		<div class="scroll">
			{#if installing}
				<ol class="steps" aria-live="polite">
					{#each u.steps as s (s.id)}
						<li class={s.state}>
							<span class="mark">
								{#if s.state === "done"}<Check size={12} strokeWidth={3} />
								{:else if s.state === "active"}<Spinner size={12} />
								{:else if s.state === "failed"}<CircleAlert size={12} />
								{:else}<i></i>{/if}
							</span>
							{s.label}
						</li>
					{/each}
				</ol>
				<button class="details" onclick={() => (detailsOpen = !detailsOpen)}>
					<ChevronRight size={12} class={detailsOpen ? "open" : ""} /> Details
				</button>
				{#if detailsOpen}
					<pre class="log mono selectable" bind:this={logEl}>{u.log.join("\n") || "starting…"}</pre>
				{/if}
			{/if}

			{#if u.notesLoading}
				<p class="muted"><Spinner size={12} /> Loading release notes…</p>
			{:else if u.notesError}
				<p class="err">Could not load release notes: {u.notesError}</p>
			{:else}
				{#each rendered as n, i (n.version)}
					<section class="release">
						{#if rendered.length > 1 || i > 0}
							<div class="rhead">
								<h3>{n.version}</h3>
								<span>{dateOf(n.published_at)}</span>
								<button
									class="link"
									onclick={() => api.openUrl(n.url)}
									aria-label="Open {n.version} on GitHub"
								>
									<ExternalLink size={11} />
								</button>
							</div>
						{/if}
						<!-- Our own CHANGELOG through the app's markdown renderer; CSP blocks inline script. -->
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
						<div class="md selectable" {onclick}>{@html n.html}</div>
					</section>
				{:else}
					<p class="muted">No release notes for this version.</p>
				{/each}
			{/if}
		</div>

		<footer>
			{#if u.relaunchNeeded}
				<Button onclick={u.relaunch}>
					{#snippet leading()}<RotateCcw size={14} />{/snippet}
					Relaunch Atlas
				</Button>
			{:else if installed}
				<Button onclick={close}>Done</Button>
			{:else}
				<Button disabled={busy || u.dev} onclick={u.updateAll}>
					{#snippet leading()}{#if busy}<Spinner size={14} />{/if}{/snippet}
					{busy ? "Updating…" : u.outcome === "failed" ? "Try again" : "Update and relaunch"}
				</Button>
				<Button variant="ghost" disabled={busy} onclick={close}>Later</Button>
				<Button variant="ghost" disabled={busy} onclick={u.skipVersion}>Skip this version</Button>
			{/if}
		</footer>
	</div>
</Drawer>

<style>
	.sheet {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}
	.hero {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 14px 16px;
		margin: 0 0 4px;
		border-radius: 12px;
		--tone: var(--ca-brand-main);
		background:
			linear-gradient(135deg, color-mix(in oklab, var(--tone) 14%, transparent), transparent 75%),
			var(--ca-surface-raised);
		box-shadow: 0 0 0 0.5px color-mix(in oklab, var(--tone) 40%, var(--ca-outline-subtle));
	}
	.hero.ok {
		--tone: var(--ca-semantic-success);
	}
	.badge {
		display: grid;
		place-items: center;
		width: 38px;
		height: 38px;
		flex-shrink: 0;
		border-radius: 10px;
		color: white;
		background: linear-gradient(160deg, color-mix(in oklab, var(--tone) 85%, white), var(--tone));
		box-shadow:
			inset 0 0.5px 0 rgb(255 255 255 / 0.35),
			0 4px 14px -4px var(--tone);
	}
	h2 {
		margin: 0;
		font-size: 16px;
		font-weight: 650;
	}
	.htext p {
		margin: 2px 0 0;
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.from {
		opacity: 0.7;
	}
	.to {
		font-weight: 650;
		color: var(--tone);
	}
	.tag {
		padding: 1px 7px;
		border-radius: 999px;
		font-size: 11px;
		font-weight: 600;
		color: var(--ca-semantic-success);
		background: color-mix(in oklab, var(--ca-semantic-success) 14%, transparent);
	}
	.scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 8px 2px 12px;
	}
	.steps {
		list-style: none;
		margin: 6px 0 4px;
		padding: 0;
		display: grid;
		gap: 8px;
	}
	.steps li {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 13px;
		color: var(--ca-text-secondary);
	}
	.steps li.active,
	.steps li.done {
		color: var(--ca-text-primary);
	}
	.steps li.failed {
		color: var(--ca-semantic-error);
	}
	.mark {
		display: grid;
		place-items: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		flex-shrink: 0;
		box-shadow: 0 0 0 1px var(--ca-outline-subtle);
		transition:
			background 160ms,
			color 160ms;
	}
	.done .mark {
		color: white;
		background: var(--ca-semantic-success);
		box-shadow: none;
	}
	.failed .mark {
		color: white;
		background: var(--ca-semantic-error);
		box-shadow: none;
	}
	.mark i {
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--ca-text-secondary);
		opacity: 0.5;
	}
	.details {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin: 8px 0 4px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.details :global(.open) {
		transform: rotate(90deg);
	}
	.log {
		max-height: 160px;
		overflow: auto;
		margin: 4px 0 12px;
		padding: 8px 10px;
		border-radius: 8px;
		font-size: 11px;
		line-height: 1.5;
		white-space: pre-wrap;
		background: var(--ca-surface-sunken, rgb(0 0 0 / 0.06));
	}
	.muted,
	.err {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.err {
		color: var(--ca-semantic-error);
	}
	.release + .release {
		margin-top: 14px;
		padding-top: 14px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.rhead {
		display: flex;
		align-items: baseline;
		gap: 8px;
		margin-bottom: 4px;
	}
	.rhead h3 {
		margin: 0;
		font-size: 14px;
		font-weight: 650;
	}
	.rhead span {
		font-size: 11.5px;
		color: var(--ca-text-secondary);
	}
	.link {
		margin-left: auto;
		display: grid;
		place-items: center;
		border: 0;
		background: none;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.md {
		font-size: 13px;
		line-height: 1.55;
		overflow-wrap: anywhere;
	}
	.md :global(h1),
	.md :global(h2) {
		font-size: 15px;
		margin: 14px 0 6px;
	}
	.md :global(h3) {
		font-size: 12px;
		margin: 14px 0 6px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--ca-text-secondary);
	}
	.md :global(h1:first-child),
	.md :global(h2:first-child),
	.md :global(h3:first-child) {
		margin-top: 0;
	}
	.md :global(p) {
		margin: 6px 0;
	}
	.md :global(ul),
	.md :global(ol) {
		margin: 4px 0 8px;
		padding-left: 18px;
	}
	.md :global(li) {
		margin: 3px 0;
	}
	.md :global(code) {
		font-family: var(--mono);
		font-size: 0.92em;
		padding: 1px 5px;
		border-radius: 5px;
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.md :global(pre.code) {
		margin: 8px 0;
		padding: 10px 12px;
		border-radius: 8px;
		overflow: auto;
		background: color-mix(in oklab, var(--ca-text-primary) 6%, transparent);
	}
	.md :global(pre.code code) {
		padding: 0;
		background: none;
	}
	.md :global(a) {
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	.md :global(a:hover) {
		text-decoration: underline;
	}
	footer {
		display: flex;
		align-items: center;
		gap: 8px;
		padding-top: 12px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
</style>
