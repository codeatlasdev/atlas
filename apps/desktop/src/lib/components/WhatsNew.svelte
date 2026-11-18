<script lang="ts">
	import { Button, Drawer, Spinner, toast } from "@codeatlasdev/ui";
	import {
		ArrowRightLeft,
		ChevronDown,
		Copy,
		ExternalLink,
		Minus,
		RotateCcw,
		ShieldCheck,
		Sparkles,
		WifiOff,
		Wrench,
	} from "@lucide/svelte";
	import type { Component, Snippet } from "svelte";
	import { api } from "#lib/api.ts";
	import Rich from "#lib/i18n/Rich.svelte";
	import { hostState, openHostPrompt } from "#lib/stores/hostStatus.svelte.ts";
	import { updates as u } from "#lib/stores/updates.svelte.ts";
	import { renderNote } from "#lib/viewers/markdown.ts";
	import {
		badgeLabel,
		badgesFor,
		channelName,
		channelOfVersion,
		dateOf,
		formatBytes,
		isLong,
		kindLabel,
		managedView,
		parseNotes,
		percent,
		prereleaseBadge,
		prereleaseLabel,
		stepLabel,
		versionAnchor,
		type GroupKind,
	} from "#lib/updates.ts";
	import { m } from "#lib/i18n/m.ts";

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

	/** A package manager owns this Atlas: its command, to copy, instead of an update that cannot finish. */
	let managed = $derived(u.outcome === "managed" && !upToDate ? managedView(u.progress.managed) : null);

	async function copy(text: string) {
		await navigator.clipboard.writeText(text);
		toast.success(m.viewers_files_copy_copied());
	}

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
	title={m.settings_updates_whats_new_title()}
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
			aria-label={m.settings_updates_whats_new_region_aria()}
		>
			{#if u.notesLoading}
				<div class="skeleton" aria-busy="true" aria-label={m.settings_updates_whats_new_loading_aria()}>
					<i style="width: 38%"></i><i style="width: 72%"></i><i style="width: 90%"></i><i style="width: 64%"
					></i>
				</div>
			{:else if u.notesError !== null}
				<div class="state" role="alert">
					<WifiOff size={22} />
					<b>{m.settings_updates_whats_new_load_failed()}</b>
					<p>{u.notesError || m.settings_updates_github_unreachable()}</p>
					<Button variant="glass" onclick={() => u.loadNotes()}>
						{#snippet leading()}<RotateCcw size={14} />{/snippet}
						{m.common_action_retry()}
					</Button>
				</div>
			{:else if releases.length === 0}
				<div class="state">
					<Sparkles size={22} />
					<b
						>{u.notesFor
							? m.settings_updates_whats_new_empty_title({ version: u.notesFor })
							: m.settings_updates_whats_new_empty_title_unknown()}</b
					>
					<p>{m.settings_updates_whats_new_empty_body()}</p>
					<Button variant="glass" onclick={() => api.openUrl(CHANGELOG_URL)}>{m.settings_updates_whats_new_open_changelog()}</Button>
				</div>
			{:else}
				{#if multi}
					<nav class="index" aria-label={m.settings_updates_whats_new_versions_aria()}>
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
									{#if r.pre}<span class="chip" data-channel={r.pre}>{prereleaseLabel(r.pre)}</span>{/if}
									{#each r.badges as b (b)}
										<span class="chip {b}">{badgeLabel(b)}</span>
									{/each}
								</div>
							</div>
							<div class="tools">
								<button
									class="icon"
									aria-label={m.settings_updates_whats_new_open_github_aria({ version: r.version })}
									onclick={() => api.openUrl(r.url)}
								>
									<ExternalLink size={13} />
								</button>
								{#if multi}
									<button
										class="icon fold"
										class:shut={!open}
										aria-expanded={open}
										aria-label={open
											? m.settings_updates_whats_new_collapse_aria({ version: r.version })
											: m.settings_updates_whats_new_expand_aria({ version: r.version })}
										onclick={() => (collapsed[r.version] = open)}
									>
										<ChevronDown size={14} />
									</button>
								{/if}
							</div>
						</header>

						{#if open}
							{#if r.parsed.summary}<p class="summary ca-selectable">
									{@html renderNote(r.parsed.summary)}
								</p>{/if}
							{#each r.parsed.groups as g, gi (g.heading)}
								{@const k = KIND[g.kind]}
								<div class="group" style="--tone: {k.tone}">
									<h3><k.icon size={13} />{kindLabel(g.kind, g.heading)}<span class="count">{g.items.length}</span></h3>
									<ul>
										{#each g.items as it, ii (ii)}
											{@const key = `${r.version}/${gi}/${ii}`}
											{@const long = isLong(it) && !unfolded[key]}
											<li class:plain={!it.title}>
												{#if it.title}<b class="title">{@html renderNote(it.title)}</b>{/if}
												{#if it.body}
													<!-- Our own CHANGELOG, sanitised by renderNote; CSP blocks inline script anyway. -->
													<div class="body ca-selectable" class:clamp={long}>
														{@html renderNote(it.body)}
													</div>
													{#if isLong(it)}
														<button class="more" aria-expanded={!long} onclick={() => (unfolded[key] = long)}>
															{long ? m.settings_updates_whats_new_fold_more() : m.settings_updates_whats_new_fold_less()}
														</button>
													{/if}
												{/if}
											</li>
										{/each}
									</ul>
								</div>
							{:else}
								<p class="quiet">{m.settings_updates_whats_new_no_changes()}</p>
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
								<span class="label">{stepLabel(s.id)}</span>
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
							<i class:indeterminate={pct === null} style:--p={pct === null ? undefined : pct / 100}></i>
						</div>
						<p class="hint">
							{pct === null
								? m.settings_updates_whats_new_download_unknown()
								: m.settings_updates_whats_new_download_progress({
										percent: pct,
										total: formatBytes(u.progress.total),
									})}
						</p>
					{/if}
					{#if u.outcome === "failed"}
						<p class="fail" role="alert">
							{u.failure ?? m.settings_updates_whats_new_update_failed()}
							{#if u.progress.rollback === "restored"}{u.info?.cli.version
									? m.settings_updates_whats_new_rollback_restored({ version: u.info.cli.version })
									: m.settings_updates_whats_new_rollback_restored_unknown()}{/if}
							{#if u.progress.rollback === "failed"}<Rich
									parts={m.settings_updates_whats_new_rollback_failed.parts()}
									tags={{ code: "code" }}
								/>{/if}
						</p>
					{/if}
					{#if hostStale}
						<p class="hint warn">
							{hostState.reason}
							{hostState.ptys.length > 0
								? m.settings_updates_whats_new_host_terminals({ count: hostState.ptys.length })
								: ""}
							<button class="inline" onclick={openHostPrompt}>{m.settings_updates_whats_new_review()}</button>
						</p>
					{/if}
					<button class="details" onclick={() => (detailsOpen = !detailsOpen)}>
						<ChevronDown size={12} class={detailsOpen ? "" : "shut"} /> {m.crew_onboarding_connect_step_details()}
					</button>
					{#if detailsOpen}
						<pre class="log mono ca-selectable" bind:this={logEl}>{u.log.join("\n") || m.settings_updates_log_starting()}</pre>
					{/if}
				</div>
			{/if}

			{#if managed?.command}
				<div class="managed">
					<p class="hint">{m.settings_updates_whats_new_managed_run({ title: managed.title })}</p>
					<div class="cmd">
						<code class="mono ca-selectable">{managed.command}</code>
						<button class="icon" aria-label={m.crew_onboarding_connect_step_copy_command()} onclick={() => copy(managed.command!)}>
							<Copy size={13} />
						</button>
					</div>
					{#if managed.installable}
						<p class="hint">{m.settings_updates_whats_new_managed_or_atlas()}</p>
					{/if}
				</div>
			{/if}
			{#if u.dev && !upToDate}
				<p class="hint">{m.settings_updates_whats_new_dev_note()}</p>
			{/if}
			<div class="actions">
				{#if u.relaunchNeeded}
					<Button onclick={u.relaunch}>
						{#snippet leading()}<RotateCcw size={14} />{/snippet}
						{m.settings_updates_whats_new_relaunch()}
					</Button>
				{:else if upToDate}
					<Button onclick={close}>{m.common_action_done()}</Button>
				{:else if managed?.installable}
					<Button disabled={busy || u.dev} onclick={u.installPackage}>
						{#snippet leading()}{#if busy}<Spinner size={14} />{/if}{/snippet}
						{busy ? m.settings_updates_updating() : m.settings_updates_whats_new_install_now({ version: latest ?? "" })}
					</Button>
				{:else}
					<Button disabled={busy || u.dev} onclick={u.updateAll}>
						{#snippet leading()}{#if busy}<Spinner size={14} />{/if}{/snippet}
						{busy
							? m.settings_updates_updating()
							: u.outcome === "failed"
								? m.common_action_retry()
								: size
									? m.settings_updates_whats_new_update_relaunch_size({ version: latest ?? "", size })
									: m.settings_updates_whats_new_update_relaunch({ version: latest ?? "" })}
					</Button>
				{/if}
			</div>
			{#if heroChannel && followed}
				<p class="hint chan">
					<Rich
						parts={m.settings_updates_whats_new_following.parts({
							channel: channelName(heroChannel),
							followed: channelName(followed),
						})}
						tags={{ chip: chanChip, b: "b" }}
					/>
				</p>
			{/if}
			<div class="links">
				{#if !upToDate && !u.relaunchNeeded}
					<button class="icon text" disabled={busy} onclick={close}>{m.settings_updates_whats_new_remind_later()}</button>
					<button class="icon text" disabled={busy} onclick={u.skipVersion}>{m.settings_updates_whats_new_skip_version()}</button>
					<span class="grow"></span>
				{/if}
				<button class="icon text" onclick={() => api.openUrl(CHANGELOG_URL)}>{m.settings_updates_full_changelog()}</button>
				<button class="icon text" onclick={() => api.openUrl(hero ? hero.url : REPO_URL)}
					>{m.settings_updates_whats_new_view_github()}</button
				>
			</div>
		</footer>
	</div>
</Drawer>

{#snippet chanChip(content: Snippet)}
	<span class="chip" data-channel={heroChannel}>{@render content()}</span>
{/snippet}

<style>
	.managed {
		margin-bottom: 8px;
	}
	.cmd {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 8px;
		border-radius: 8px;
		background: var(--ca-surface-sunken, rgb(0 0 0 / 0.2));
	}
	.cmd code {
		flex: 1;
		min-width: 0;
		overflow-wrap: anywhere;
		font-size: 12px;
	}
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
		container-type: inline-size;
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
		/* A version number is one word: when the sheet is narrow it shrinks, it is never cut in the middle. */
		white-space: nowrap;
	}
	.first .num {
		font-size: clamp(20px, 8cqw, 30px);
		font-weight: 700;
		line-height: 1.05;
		letter-spacing: -0.03em;
	}
	.ver {
		min-width: 0;
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
	/*
	 * A badge is a self-contained pill: its box never depends on the block it sits in. The channel is a data attribute,
	 * not a class: `alpha` is also the footer note's class, and that rule's margin/padding/border once turned the badge
	 * into a tall pill below the date.
	 */
	.chip {
		--tone: var(--ca-text-secondary);
		display: inline-flex;
		align-items: center;
		flex: none;
		box-sizing: border-box;
		max-width: 100%;
		height: 19px;
		padding: 0 7px;
		border-radius: 999px;
		font-size: 11px;
		font-weight: 600;
		line-height: 1;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
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
	.chip[data-channel="alpha"] {
		--tone: var(--ca-semantic-error);
	}
	.chip[data-channel="beta"] {
		--tone: var(--ca-semantic-warning);
	}
	.chip[data-channel="rc"] {
		--tone: var(--ca-semantic-info);
	}
	.chip[data-channel="stable"] {
		--tone: var(--ca-semantic-success);
	}
	.hint.chan {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 4px 6px;
		margin-top: 8px;
		line-height: 19px;
	}
	.hint.chan .chip {
		text-transform: capitalize;
	}
	.hint.chan b {
		min-width: 0;
		overflow-wrap: anywhere;
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
	/* Inline markdown the sheet renders itself: the intro, item titles, bodies and the footer note share one look. */
	.summary :global(code),
	.title :global(code),
	.body :global(code),
	.alpha :global(code) {
		padding: 1px 5px;
		border-radius: 5px;
		font-family: var(--mono);
		font-size: 0.9em;
		color: var(--ca-text-primary);
		background: color-mix(in oklab, var(--ca-text-primary) 8%, transparent);
	}
	.summary :global(strong),
	.alpha :global(strong) {
		font-weight: 650;
	}
	.summary :global(a),
	.body :global(a),
	.alpha :global(a) {
		color: var(--ca-brand-main);
		text-decoration: none;
	}
	.summary :global(a:hover),
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
		width: 100%;
		height: 100%;
		border-radius: inherit;
		background: var(--ca-brand-main);
		/* Progress as scaleX: a width transition would relayout the dialog footer on every chunk. */
		transform-origin: left;
		transform: scaleX(var(--p, 0));
		transition: transform 160ms linear;
	}
	.bar i.indeterminate {
		width: 30%;
		transform: none;
		transition: none;
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
	/* The primary button ("Update to 1.5.0-alpha.25 and relaunch · 27.0 MB") is the longest label in the sheet: it fills
	   the row and, when the sheet is narrow, wraps its label instead of overflowing the edge. */
	.actions > :global(button) {
		flex: 1 1 0;
		min-width: 0;
		max-width: 100%;
		height: auto;
		min-height: 36px;
		padding-block: 6px;
		white-space: normal;
		text-wrap: balance;
		line-height: 1.25;
	}
	.links {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 2px 2px;
		margin: 8px 0 0 -6px;
	}
	.links .icon.text {
		height: auto;
		min-height: 26px;
		white-space: nowrap;
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
