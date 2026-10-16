<script lang="ts">
	import { Button, Drawer, Spinner } from "@codeatlasdev/ui";
	import {
		ArrowRightLeft,
		ChevronDown,
		ExternalLink,
		Minus,
		RotateCcw,
		ShieldCheck,
		Sparkles,
		WifiOff,
		Wrench,
	} from "@lucide/svelte";
	import type { Component } from "svelte";
	import { api } from "#lib/api.ts";
	import { hostState, openHostPrompt } from "#lib/stores/hostStatus.svelte.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import { renderNote } from "#lib/viewers/markdown.ts";
	import {
		badgesFor,
		dateOf,
		formatBytes,
		isLong,
		channelOfVersion,
		parseNotes,
		percent,
		prereleaseBadge,
		versionAnchor,
		type GroupKind,
	} from "#lib/updates.ts";

	const CHANGELOG_URL = "https://atlas.codeatlas.com.br/changelog";
	const REPO_URL = "https://github.com/codeatlasdev/atlas";

	const KIND: Record<GroupKind, { icon: Component; tone: string }> = {
		added: { icon: Sparkles, tone: "var(--ca-semantic-success)" },
		changed: { icon: ArrowRightLeft, tone: "var(--ca-semantic-info)" },
		fixed: { icon: Wrench, tone: "var(--ca-semantic-warning)" },
		security: { icon: ShieldCheck, tone: "var(--ca-semantic-error)" },
		removed: { icon: Minus, tone: "var(--ca-text-secondary)" },
		deprecated: { icon: Minus, tone: "var(--ca-text-secondary)" },
		other: { icon: Sparkles, tone: "var(--ca-brand-main)" },
	};

	let detailsOpen = $state(false);
	let logEl: HTMLPreElement | undefined = $state();
	let scrollEl: HTMLDivElement | undefined = $state();
	/** Items whose long body the reader unfolded, and versions they collapsed (`version/group/index`). */
	let unfolded = $state<Record<string, boolean>>({});
	let collapsed = $state<Record<string, boolean>>({});
	let active = $state("");

	let running = $derived(u.info?.app.version);
	let latest = $derived(u.release?.version);
	/** An install is under way or has just ended: the footer and the steps take over. */
	let installing = $derived(u.running === "update" || u.outcome === "done" || u.outcome === "failed");
	let busy = $derived(u.running === "update");
	/** Nothing to install: the sheet shows the notes of the running (or just installed) version. */
	let upToDate = $derived(!u.available || !!u.updatedTo);
	let releases = $derived(
		u.notes.map((n) => ({
			...n,
			parsed: parseNotes(n.notes),
			badges: badgesFor(n.version, running, latest, !!u.updatedTo),
			pre: prereleaseBadge(n.version),
			date: dateOf(n.published_at),
		})),
	);
	let multi = $derived(releases.length > 1);
	let hero = $derived(releases[0]);
	let alpha = $derived(releases.find((r) => r.parsed.alpha)?.parsed.alpha ?? "");
	/** Channel the shown release was cut for, and the one this app follows (footer line). */
	let heroChannel = $derived(hero ? channelOfVersion(hero.version) : null);
	let followed = $derived(u.channelState?.channel ?? null);
	let pct = $derived(percent(u.progress));
	let downloading = $derived(busy && u.steps[0].state === "active" && u.progress.at <= 0);
	let size = $derived(formatBytes(u.size ?? 0));
	/** The update ended but the terminal host is still stale: say so here, with the way to fix it. */
	let hostStale = $derived(
		installing && u.outcome === "done" && hostState.skew === "needsRestart" && hostState.reason !== "",
	);

	$effect(() => {
		u.log.length;
		if (logEl) logEl.scrollTop = logEl.scrollHeight;
	});

	/** The version index follows the scroll: the topmost block that has passed the header. */
	function track() {
		if (!scrollEl || !multi) return;
		const top = scrollEl.getBoundingClientRect().top + 64;
		let cur = releases[0]?.version ?? "";
		for (const r of releases) {
			const el = scrollEl.querySelector<HTMLElement>(`#${versionAnchor(r.version)}`);
			if (el && el.getBoundingClientRect().top <= top) cur = r.version;
		}
		active = cur;
	}

	function jump(version: string) {
		collapsed[version] = false;
		const el = scrollEl?.querySelector<HTMLElement>(`#${versionAnchor(version)}`);
		const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
		el?.scrollIntoView({ block: "start", behavior: reduce ? "auto" : "smooth" });
		active = version;
	}

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

	/** `j`/`k` or the arrows move between versions, `Enter` on a header folds it. */
	function onkeydown(e: KeyboardEvent) {
		if (!multi || (e.target as HTMLElement).closest("button, a, pre")) return;
		const i = releases.findIndex((r) => r.version === (active || releases[0].version));
		const to =
			e.key === "j" || e.key === "ArrowDown" ? i + 1 : e.key === "k" || e.key === "ArrowUp" ? i - 1 : i;
		if (to === i || !releases[to]) return;
		e.preventDefault();
		jump(releases[to].version);
	}
</script>

<Drawer
	bind:open={u.whatsNewOpen}
	side="right"
	width="520px"
	title="What's new"
	onOpenChange={(o) => !o && u.dismissUpdated()}
>
	<div class="sheet">
		<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
		<div
			class="scroll"
			bind:this={scrollEl}
			onscroll={track}
			{onkeydown}
			tabindex="0"
			role="region"
			aria-label="Release notes"
		>
			{#if u.notesLoading}
				<div class="skeleton" aria-busy="true" aria-label="Loading release notes">
					<i style="width: 38%"></i><i style="width: 72%"></i><i style="width: 90%"></i><i style="width: 64%"
					></i>
				</div>
			{:else if u.notesError}
				<div class="state" role="alert">
					<WifiOff size={22} />
					<b>Could not load the release notes</b>
					<p>{u.notesError}</p>
					<Button variant="glass" onclick={() => u.loadNotes()}>
						{#snippet leading()}<RotateCcw size={14} />{/snippet}
						Retry
					</Button>
				</div>
			{:else if releases.length === 0}
				<div class="state">
					<Sparkles size={22} />
					<b>No release notes for this version</b>
					<p>The changelog has no entry for it yet.</p>
					<Button variant="glass" onclick={() => api.openUrl(CHANGELOG_URL)}>Open the full changelog</Button>
				</div>
			{:else}
				{#if multi}
					<nav class="index" aria-label="Versions">
						{#each releases as r (r.version)}
							<button
								class:on={active === r.version || (!active && r === releases[0])}
								onclick={() => jump(r.version)}
							>
								{r.version}
							</button>
						{/each}
					</nav>
				{/if}

				{#each releases as r, ri (r.version)}
					{@const open = !collapsed[r.version]}
					<section class="release" class:first={ri === 0} id={versionAnchor(r.version)}>
						<header class="head">
							<div class="ver">
								<h2 class="num">{r.version}</h2>
								<div class="meta">
									{#if r.date}<time>{r.date}</time>{/if}
									{#if r.pre}<span class="chip pre {r.pre.toLowerCase()}">{r.pre}</span>{/if}
									{#each r.badges as b (b)}
										<span class="chip {b}"
											>{b === "latest" ? "Latest" : b === "new" ? "New" : "Installed"}</span
										>
									{/each}
								</div>
							</div>
							<div class="tools">
								<button
									class="icon"
									aria-label="Open {r.version} on GitHub"
									onclick={() => api.openUrl(r.url)}
								>
									<ExternalLink size={13} />
								</button>
								{#if multi}
									<button
										class="icon fold"
										class:shut={!open}
										aria-expanded={open}
										aria-label="{open ? 'Collapse' : 'Expand'} {r.version}"
										onclick={() => (collapsed[r.version] = open)}
									>
										<ChevronDown size={14} />
									</button>
								{/if}
							</div>
						</header>

						{#if open}
							{#if r.parsed.summary}<p class="summary">{r.parsed.summary}</p>{/if}
							{#each r.parsed.groups as g, gi (g.heading)}
								{@const k = KIND[g.kind]}
								<div class="group" style="--tone: {k.tone}">
									<h3><k.icon size={13} />{g.heading}<span class="count">{g.items.length}</span></h3>
									<ul>
										{#each g.items as it, ii (ii)}
											{@const key = `${r.version}/${gi}/${ii}`}
											{@const long = isLong(it) && !unfolded[key]}
											<li class:plain={!it.title}>
												{#if it.title}<b class="title">{it.title}</b>{/if}
												{#if it.body}
													<!-- Our own CHANGELOG, sanitised by renderNote; CSP blocks inline script anyway. -->
													<div class="body ca-selectable" class:clamp={long}>
														{@html renderNote(it.body)}
													</div>
													{#if isLong(it)}
														<button class="more" aria-expanded={!long} onclick={() => (unfolded[key] = long)}>
															{long ? "More" : "Less"}
														</button>
													{/if}
												{/if}
											</li>
										{/each}
									</ul>
								</div>
							{:else}
								<p class="quiet">No changes listed for this version.</p>
							{/each}
						{/if}
					</section>
				{/each}

				{#if alpha}
					<p class="alpha">{@html renderNote(alpha)}</p>
				{/if}
			{/if}
		</div>

		<footer>
			{#if installing}
				<div class="install" aria-live="polite">
					<ol class="steps">
						{#each u.steps as s (s.id)}
							<li class={s.state}>
								<span class="label">{s.label}</span>
							</li>
						{/each}
					</ol>
					{#if downloading}
						<div
							class="bar"
							role="progressbar"
							aria-valuemin="0"
							aria-valuemax="100"
							aria-valuenow={pct ?? undefined}
						>
							<i class:indeterminate={pct === null} style:width={pct === null ? undefined : `${pct}%`}></i>
						</div>
						<p class="hint">
							{pct === null ? "Downloading…" : `${pct}%`}{u.progress.total
								? ` of ${formatBytes(u.progress.total)}`
								: ""}
						</p>
					{/if}
					{#if u.outcome === "failed"}
						<p class="fail" role="alert">
							{u.failure ?? "The update failed."}
							{#if u.progress.rollback === "restored"}Atlas CLI stays on {u.info?.cli.version ??
									"its previous version"}; nothing is half updated.{/if}
							{#if u.progress.rollback === "failed"}The CLI could not be rolled back: run <code
									>atlas desktop --install</code
								>.{/if}
						</p>
					{/if}
					{#if hostStale}
						<p class="hint warn">
							{hostState.reason}
							{hostState.ptys.length > 0
								? `${hostState.ptys.length} terminal${hostState.ptys.length === 1 ? " is" : "s are"} still running, so Atlas did not restart it.`
								: ""}
							<button class="inline" onclick={openHostPrompt}>Review</button>
						</p>
					{/if}
					<button class="details" onclick={() => (detailsOpen = !detailsOpen)}>
						<ChevronDown size={12} class={detailsOpen ? "" : "shut"} /> Details
					</button>
					{#if detailsOpen}
						<pre class="log mono ca-selectable" bind:this={logEl}>{u.log.join("\n") || "starting…"}</pre>
					{/if}
				</div>
			{/if}

			{#if u.dev && !upToDate}
				<p class="hint">This is a dev build; Atlas will not replace it with a release.</p>
			{/if}
			<div class="actions">
				{#if u.relaunchNeeded}
					<Button onclick={u.relaunch}>
						{#snippet leading()}<RotateCcw size={14} />{/snippet}
						Relaunch Atlas
					</Button>
				{:else if upToDate}
					<Button onclick={close}>Done</Button>
				{:else}
					<Button disabled={busy || u.dev} onclick={u.updateAll}>
						{#snippet leading()}{#if busy}<Spinner size={14} />{/if}{/snippet}
						{busy
							? "Updating…"
							: u.outcome === "failed"
								? "Try again"
								: `Update to ${latest} and relaunch${size ? ` · ${size}` : ""}`}
					</Button>
				{/if}
			</div>
			{#if heroChannel && followed}
				<p class="hint chan">
					<span class="chip pre {heroChannel}">{heroChannel}</span>
					release · following <b>{followed}</b>
				</p>
			{/if}
			<div class="links">
				{#if !upToDate && !u.relaunchNeeded}
					<button class="icon text" disabled={busy} onclick={close}>Remind me later</button>
					<button class="icon text" disabled={busy} onclick={u.skipVersion}>Skip this version</button>
					<span class="grow"></span>
				{/if}
				<button class="icon text" onclick={() => api.openUrl(CHANGELOG_URL)}>Full changelog</button>
				<button class="icon text" onclick={() => api.openUrl(hero ? hero.url : REPO_URL)}
					>View on GitHub</button
				>
			</div>
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
	.scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 2px 4px 20px 2px;
		scroll-padding-top: 52px;
		outline: none;
	}

	/* Version index: sticky, one chip per version. */
	.index {
		position: sticky;
		top: 2px;
		z-index: 2;
		display: flex;
		gap: 4px;
		margin: 0 0 14px;
		padding: 4px;
		overflow-x: auto;
		scrollbar-width: none;
		background: var(--ca-surface-raised);
		border-radius: 10px;
		box-shadow:
			0 0 0 0.5px var(--ca-outline-subtle),
			0 8px 12px -8px rgb(0 0 0 / 0.35);
	}
	.index button {
		flex: none;
		padding: 3px 9px;
		border: 0;
		border-radius: 999px;
		font: inherit;
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		color: var(--ca-text-secondary);
		background: none;
		cursor: pointer;
		transition:
			background 140ms,
			color 140ms;
	}
	.index button:hover {
		background: var(--ca-action-hover);
	}
	.index button.on {
		color: var(--ca-text-primary);
		font-weight: 600;
		background: color-mix(in oklab, var(--ca-brand-main) 16%, transparent);
	}

	/* One release: a header, an optional summary, grouped changes. */
	.release {
		position: relative;
	}
	.release + .release {
		margin-top: 22px;
		padding-top: 18px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
	}
	.num {
		margin: 0;
		font-size: 15px;
		font-weight: 650;
		font-variant-numeric: tabular-nums;
		letter-spacing: -0.01em;
	}
	.first .num {
		font-size: 30px;
		font-weight: 700;
		line-height: 1.05;
		letter-spacing: -0.03em;
	}
	.meta {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 4px;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.chip {
		padding: 1px 7px;
		border-radius: 999px;
		font-size: 11px;
		font-weight: 600;
		color: var(--tone);
		background: color-mix(in oklab, var(--tone) 14%, transparent);
	}
	.chip.latest {
		--tone: var(--ca-brand-main);
	}
	.chip.new {
		--tone: var(--ca-semantic-success);
	}
	.chip.installed {
		--tone: var(--ca-text-secondary);
	}
	.chip.pre.alpha {
		--tone: var(--ca-semantic-error);
	}
	.chip.pre.beta {
		--tone: var(--ca-semantic-warning);
	}
	.chip.pre.rc {
		--tone: var(--ca-semantic-info);
	}
	.chip.pre.stable {
		--tone: var(--ca-semantic-success);
	}
	.hint.chan {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 8px;
	}
	.hint.chan .chip {
		text-transform: capitalize;
	}
	.hint.chan b {
		color: var(--ca-text-primary);
		text-transform: capitalize;
	}
	.tools {
		display: flex;
		gap: 2px;
		margin-top: 2px;
	}
	.icon {
		display: inline-grid;
		place-items: center;
		min-width: 26px;
		height: 26px;
		padding: 0 6px;
		border: 0;
		border-radius: 7px;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		background: none;
		cursor: pointer;
	}
	.icon:hover {
		color: var(--ca-text-primary);
		background: var(--ca-action-hover);
	}
	.icon.text {
		display: inline-flex;
		align-items: center;
	}
	.fold :global(svg) {
		transition: transform 160ms;
	}
	.fold.shut :global(svg) {
		transform: rotate(-90deg);
	}
	.summary {
		margin: 12px 0 2px;
		font-size: 14px;
		line-height: 1.5;
		color: var(--ca-text-primary);
		text-wrap: pretty;
	}

	/* A group of changes: a coloured spine and header carry the kind; items are plain text. */
	.group {
		margin-top: 16px;
		padding-left: 12px;
		border-left: 2px solid color-mix(in oklab, var(--tone) 55%, transparent);
	}
	.group h3 {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0 0 8px;
		font-size: 13px;
		font-weight: 650;
		color: var(--tone);
	}
	.count {
		margin-left: 2px;
		font-size: 11px;
		font-weight: 500;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
	}
	ul {
		display: grid;
		gap: 16px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		font-size: 13px;
		line-height: 1.55;
		overflow-wrap: anywhere;
	}
	li.plain {
		position: relative;
		padding-left: 14px;
	}
	li.plain::before {
		content: "";
		position: absolute;
		left: 2px;
		top: 0.68em;
		width: 4px;
		height: 4px;
		border-radius: 50%;
		background: var(--tone);
		opacity: 0.8;
	}
	li.plain .body {
		color: var(--ca-text-primary);
	}
	.title {
		display: block;
		font-weight: 620;
		color: var(--ca-text-primary);
	}
	.body {
		color: var(--ca-text-secondary);
		text-wrap: pretty;
	}
	.body.clamp {
		display: -webkit-box;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		overflow: hidden;
	}
	.body :global(p) {
		margin: 0;
	}
	.body :global(ul) {
		display: block;
		margin: 4px 0 0;
		padding-left: 16px;
		list-style: disc;
	}
	.body :global(li) {
		margin: 2px 0;
		font-size: inherit;
	}
	.body :global(code) {
		padding: 1px 5px;
		border-radius: 5px;
		font-family: var(--mono);
		font-size: 0.9em;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.body :global(a),
	.alpha :global(a) {
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	.body :global(a:hover),
	.alpha :global(a:hover) {
		text-decoration: underline;
	}
	.more {
		margin-top: 2px;
		padding: 0;
		border: 0;
		font: inherit;
		font-size: 12px;
		font-weight: 600;
		color: var(--ca-brand-main);
		background: none;
		cursor: pointer;
	}
	.quiet {
		margin: 14px 0 0;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
	}
	.alpha {
		margin: 26px 0 0;
		padding-top: 12px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		font-size: 11.5px;
		line-height: 1.5;
		color: var(--ca-text-secondary);
	}

	/* Empty, error and loading states. */
	.state {
		display: grid;
		justify-items: center;
		gap: 8px;
		padding: 48px 16px;
		text-align: center;
		color: var(--ca-text-secondary);
	}
	.state b {
		color: var(--ca-text-primary);
		font-size: 14px;
	}
	.state p {
		margin: 0 0 6px;
		max-width: 38ch;
		font-size: 12.5px;
	}
	.skeleton {
		display: grid;
		gap: 12px;
		padding-top: 6px;
	}
	.skeleton i {
		display: block;
		height: 12px;
		border-radius: 6px;
		background: color-mix(in oklab, var(--ca-text-primary) 9%, transparent);
		animation: pulse 1.4s ease-in-out infinite;
	}
	.skeleton i:first-child {
		height: 28px;
	}
	@keyframes pulse {
		50% {
			opacity: 0.45;
		}
	}

	/* Footer: install progress above the actions. */
	footer {
		padding-top: 12px;
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.install {
		display: grid;
		gap: 8px;
		margin-bottom: 12px;
	}
	.steps {
		display: flex;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.steps li {
		flex: 1;
		min-width: 0;
		display: grid;
		gap: 5px;
		font-size: 11px;
		color: var(--ca-text-secondary);
	}
	.steps li::before {
		content: "";
		height: 3px;
		border-radius: 2px;
		background: color-mix(in oklab, var(--ca-text-primary) 12%, transparent);
		transition: background 200ms;
	}
	.steps li.done::before {
		background: var(--ca-semantic-success);
	}
	.steps li.active::before {
		background: var(--ca-brand-main);
	}
	.steps li.failed::before {
		background: var(--ca-semantic-error);
	}
	.steps li.active,
	.steps li.done {
		color: var(--ca-text-primary);
	}
	.steps li.failed {
		color: var(--ca-semantic-error);
	}
	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.bar {
		height: 4px;
		overflow: hidden;
		border-radius: 2px;
		background: color-mix(in oklab, var(--ca-text-primary) 10%, transparent);
	}
	.bar i {
		display: block;
		height: 100%;
		border-radius: inherit;
		background: var(--ca-brand-main);
		transition: width 160ms linear;
	}
	.bar i.indeterminate {
		width: 30%;
		animation: slide 1.2s ease-in-out infinite;
	}
	@keyframes slide {
		from {
			transform: translateX(-100%);
		}
		to {
			transform: translateX(340%);
		}
	}
	.hint,
	.fail {
		margin: 0;
		font-size: 12px;
		color: var(--ca-text-secondary);
	}
	.inline {
		padding: 0;
		border: 0;
		font: inherit;
		font-weight: 600;
		color: var(--ca-brand-main);
		background: none;
		cursor: pointer;
	}
	.hint.warn {
		color: var(--ca-semantic-warning);
	}
	.fail {
		color: var(--ca-semantic-error);
	}
	.fail code {
		font-family: var(--mono);
	}
	.details {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		justify-self: start;
		padding: 0;
		border: 0;
		font: inherit;
		font-size: 12px;
		color: var(--ca-text-secondary);
		background: none;
		cursor: pointer;
	}
	.details :global(.shut) {
		transform: rotate(-90deg);
	}
	.log {
		max-height: 140px;
		overflow: auto;
		margin: 0;
		padding: 8px 10px;
		border-radius: 8px;
		font-size: 11px;
		line-height: 1.5;
		white-space: pre-wrap;
		background: var(--ca-surface-sunken, rgb(0 0 0 / 0.06));
	}
	.actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}
	.actions > :global(button) {
		flex: 1;
	}
	.links {
		display: flex;
		align-items: center;
		gap: 2px;
		margin: 8px 0 0 -6px;
	}
	.grow {
		flex: 1;
	}

	@media (prefers-reduced-motion: reduce) {
		.skeleton i,
		.bar i.indeterminate {
			animation: none;
		}
		.bar i,
		.steps li::before,
		.fold :global(svg),
		.index button {
			transition: none;
		}
	}
</style>
