<script lang="ts">
	import { onDestroy, untrack } from "svelte";
	import { primaryKey } from "#lib/keys.ts";
	import { toast } from "@codeatlasdev/ui";
	import { ArrowDown } from "@lucide/svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { activity } from "#lib/panel/activity.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import type { Level } from "#lib/logs/model.ts";
	import {
		ALL_TIME,
		NO_WINDOW,
		rangeLabel,
		resolveWindow,
		slideCutoff,
		windowHides,
		type TimeRange,
		type TimeWindow,
	} from "#lib/logs/range.ts";
	import { compileQuery, type Filter } from "#lib/logs/view.ts";
	import { jumpLevel, rowAtTime } from "#lib/logs/search.ts";
	import { resolvePath } from "#lib/logs/links.ts";
	import { format } from "#lib/logs/text.ts";
	import type { Seg } from "#lib/logs/segments.ts";
	import { LogsEngine } from "#lib/logs/engine.svelte.ts";
	import { LogsViewport, ROW } from "#lib/logs/viewport.svelte.ts";
	import { handleLogsKey } from "#lib/logs/keys.ts";
	import type { LogLine } from "#lib/logs/model.ts";
	import LogsFilterBar from "./logs/LogsFilterBar.svelte";
	import LogsSearchBar from "./logs/LogsSearchBar.svelte";
	import LogsRangeChip from "./logs/LogsRangeChip.svelte";
	import LogsEmpty from "./logs/LogsEmpty.svelte";
	import LogsStatus from "./logs/LogsStatus.svelte";
	import LogsTools from "./logs/LogsTools.svelte";
	import LogRow from "./logs/LogRow.svelte";

	let { project, worktreeId }: { project: ProjectView; worktreeId?: string } = $props();

	/** Lines handed to the clipboard by "Copy for AI". */
	const AI_LINES = 300;

	// ───────── controls ─────────
	let only = $state<string | null>(store.focusedService);
	let minLevel = $state<Level>("info");
	let query = $state("");
	let useRegex = $state(false);
	let matchCase = $state(false);
	let hideNonMatching = $state(false);

	let live = $derived(store.live[project.id]);
	let names = $derived(
		[
			...new Set([
				...(store.configs[project.id]?.services.map((s) => s.name) ?? []),
				...(live?.services.map((s) => s.name) ?? []),
			]),
		].sort(),
	);
	let serviceDir = $derived(
		Object.fromEntries((store.configs[project.id]?.services ?? []).map((s) => [s.name, s.dir])),
	);
	// The focused service comes from whichever project was open before: drop it when
	// this project has no such service (it would hide every line).
	$effect(() => {
		if (only && names.length && !names.includes(only)) only = null;
	});

	let compiled = $derived(compileQuery(query, useRegex, matchCase));
	let matcher = $derived(compiled === "bad" ? null : compiled);
	let badRegex = $derived(compiled === "bad");
	// Time range: `win` is the range resolved against the ring (a restart marker moves `minAbs`);
	// a sliding "last N min" keeps only its length, its cutoff is taken from the clock when lines are tested.
	let range = $state<TimeRange>(ALL_TIME);
	let win = $state.raw<TimeWindow>(NO_WINDOW);
	let filter = $derived<Filter>({ only, minLevel, matcher, hideNonMatching, win });
	let rangeText = $derived(rangeLabel(range));

	// ───────── incremental view ─────────
	const eng = new LogsEngine(store.logs(untrack(() => project.id)));
	const vp = new LogsViewport(eng, () => !!only);
	let loading = $derived(project.running && !live?.logsLoaded);

	onDestroy(() => eng.destroy());

	/** The filter with the sliding cutoff of this very moment (applied to lines as they are tested). */
	function withClock(f: Filter): Filter {
		return { ...f, cutoff: slideCutoff(f.win, Date.now()) };
	}
	const rebuild = () => eng.rebuild(withClock(filter), matcher);
	eng.onFirstMatch = () => step(1);

	/** Filter changes (service, level, query mode, time range) → full, time-sliced rebuild. */
	$effect(() => {
		void filter;
		untrack(rebuild);
	});

	/**
	 * Re-resolve the range against the ring. A moved boundary (a restart arrived, the range or the
	 * service changed, a new day for a custom range) changes `win`, which rebuilds the view once.
	 */
	function resolveRange() {
		const next = resolveWindow(range, eng.ring, only, Date.now());
		if (
			next.minAbs !== win.minAbs ||
			next.lastMs !== win.lastMs ||
			next.fromMs !== win.fromMs ||
			next.toMs !== win.toMs
		)
			win = next;
	}
	$effect(() => {
		void range;
		void only;
		untrack(resolveRange);
	});
	// A restart marker moves "since restart": check on every batch, but only that range scans.
	$effect(() => {
		void live?.logRev;
		if (untrack(() => range.kind) === "restart") untrack(resolveRange);
	});

	/** A sliding window ages lines out by itself: once a second, drop what fell below the cutoff. */
	$effect(() => {
		if (!win.lastMs) return;
		const t = setInterval(() => eng.slide(slideCutoff(win, Date.now()), rebuild), 1000);
		return () => clearInterval(t);
	});

	/** New lines → append to the view; a replaced ring (resync) → rebuild. */
	$effect(() => {
		void live?.logRev;
		untrack(() => eng.ingest(store.logs(project.id), () => withClock(filter), rebuild));
	});

	$effect(() => {
		void eng.rev;
		void vp.contentH;
		untrack(() => vp.followTail());
	});

	// "Open logs at that moment" from the Activity drawer: clear a time range that would hide the
	// moment (with a toast), narrow to the service, then center the first line at/after the time once
	// the (time-sliced) rebuild has caught up.
	$effect(() => {
		const s = activity.seek;
		if (!s || s.project !== project.id) return;
		void eng.rev;
		untrack(() => {
			if (range.kind !== "all" && windowHides(win, eng.ring, s.ts, Date.now())) {
				toast.info(`Cleared "${rangeText}" to show that moment`);
				range = ALL_TIME;
				return;
			}
			if (only !== s.service) {
				only = s.service;
				return;
			}
			if (eng.refilter || !vp.el) return;
			activity.seek = null;
			const r = rowAtTime(eng.view, eng.ring, s.ts);
			if (r >= 0) vp.scrollToRow(r);
		});
	});

	// Measure the monospace advance once and whenever the font changes.
	let probe: HTMLSpanElement | undefined = $state();
	$effect(() => {
		void eng.rev;
		if (probe) vp.measure(probe);
	});

	// ───────── search navigation ─────────
	let searchInput: HTMLInputElement | undefined = $state();
	let curOrdinal = $derived(
		eng.search && eng.cur >= 0 ? (void eng.matchCount, eng.search.ordinal(eng.cur)) : 0,
	);

	function step(dir: 1 | -1) {
		const { search } = eng;
		if (!search) return;
		if (!search.rows.length) {
			// Matches may still be coming in while the scan runs.
			eng.pendingJump = eng.searching;
			return;
		}
		const from = eng.cur >= 0 ? eng.cur : dir === 1 ? -1 : eng.view.length;
		const r = dir === 1 ? search.next(from) : search.prev(from);
		if (r < 0) return;
		eng.cur = r;
		vp.scrollToRow(r);
	}

	function level(dir: 1 | -1, min: Level) {
		const from = eng.cur >= 0 ? eng.cur : vp.follow ? eng.view.length : Math.max(0, vp.rowAt(vp.scrollTop));
		const r = jumpLevel(eng.view, eng.ring, from, dir, min);
		if (r < 0) return toast.info(`No ${min} lines`);
		eng.cur = r;
		vp.scrollToRow(r);
	}

	function focusSearch() {
		searchInput?.focus();
		searchInput?.select();
	}

	function onkeydown(e: KeyboardEvent) {
		handleLogsKey(e, {
			viewport: vp.el,
			height: vp.height,
			focusSearch,
			step,
			level,
			toBottom: () => vp.toBottom(),
			setFollow: (on) => (vp.follow = on),
			toggleFollow: () => {
				vp.follow = !vp.follow;
				if (vp.follow) vp.toBottom();
			},
			toggleTime: () => (vp.showTime = !vp.showTime),
			escape: () => {
				if (query) query = "";
				else if (eng.cur >= 0) eng.cur = -1;
			},
		});
	}

	// ───────── links ─────────
	const wtId = $derived(worktreeId ?? ws.selected?.id);
	/** File links resolve against the tab's worktree checkout (a linked worktree has its own path). */
	const root = $derived(ws.worktree(wtId)?.path ?? project.root);

	async function openLink(e: MouseEvent, seg: Seg, l: LogLine) {
		const link = seg.link;
		if (!link || !primaryKey(e)) return;
		e.preventDefault();
		e.stopPropagation();
		try {
			if (link.kind === "url" || link.kind === "osc") {
				const url = link.target;
				if (/^https?:/i.test(url) && wtId) await openPreview(wtId, url, e.shiftKey);
				else await api.openUrl(url);
				return;
			}
			const rel = resolvePath(link.target, root, serviceDir[l.name]);
			if (rel && wtId) openFile(wtId, rel, link.line, link.col);
			else toast.error(`${link.target} is outside the project`);
		} catch (err) {
			toast.error(err instanceof Error ? err.message : String(err));
		}
	}

	let metaDown = $state(false);

	// ───────── copy ─────────
	async function copyVisible() {
		const lines = eng.viewLines();
		await navigator.clipboard.writeText(format(lines, vp.showTime));
		toast.success(`Copied ${lines.length.toLocaleString()} lines`);
	}

	/** Last lines of the filtered view with a header that tells a model what it is looking at. */
	async function copyForAi() {
		const lines = eng.viewLines(AI_LINES);
		const scope = only ?? "all services";
		const head = `Logs of ${project.name} (${scope}${minLevel !== "info" ? `, ${minLevel}+` : ""}${query ? `, matching "${query}"` : ""}), last ${lines.length} lines:`;
		await navigator.clipboard.writeText(`${head}\n\`\`\`\n${format(lines, vp.showTime)}\n\`\`\``);
		toast.success(`Copied ${lines.length} lines for AI`);
	}

	/** Copying a selection keeps the plain `text`, not the decorated DOM (timestamps, labels). */
	function oncopy(e: ClipboardEvent) {
		const sel = getSelection();
		if (!sel || sel.isCollapsed || !e.clipboardData) return;
		if (!vp.el?.contains(sel.anchorNode)) return;
		// The timestamp and service label are `user-select: none`; flex rows still emit blank lines between rows.
		e.clipboardData.setData("text/plain", sel.toString().replace(/\n{2,}/g, "\n"));
		e.preventDefault();
	}

	let rangeActive = $derived(range.kind !== "all");
	let narrowed = $derived(only || minLevel !== "info" || rangeActive || (hideNonMatching && matcher));
	function resetFilters() {
		query = "";
		only = null;
		minLevel = "info";
		range = ALL_TIME;
	}
</script>

<svelte:window
	onkeydown={(e) => (metaDown = primaryKey(e))}
	onkeyup={(e) => (metaDown = primaryKey(e))}
	onblur={() => (metaDown = false)}
/>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="logs" {onkeydown} {oncopy}>
	<LogsFilterBar {names} counts={eng.counts} bind:only bind:minLevel />

	<div class="tools">
		<LogsSearchBar
			bind:query
			bind:matchCase
			bind:useRegex
			bind:hideNonMatching
			bind:input={searchInput}
			{badRegex}
			matchCount={eng.matchCount}
			{curOrdinal}
			searching={eng.searching}
			onstep={step}
			onescape={() => vp.el?.focus()}
		/>
		<LogsRangeChip bind:range {only} />
		<LogsTools
			bind:showTime={vp.showTime}
			bind:wrap={vp.wrap}
			bind:follow={vp.follow}
			aiLines={AI_LINES}
			onjump={level}
			onfollow={() => vp.toBottom()}
			oncopy={copyVisible}
			oncopyai={copyForAi}
		/>
	</div>

	<div class="body">
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="viewport"
			class:meta={metaDown}
			bind:this={vp.el}
			bind:clientHeight={vp.height}
			bind:clientWidth={vp.width}
			onscroll={vp.onscroll}
			tabindex="0"
			role="log"
			aria-label="Service logs"
			aria-live="off"
		>
			<span class="probe" bind:this={probe} aria-hidden="true">{"0".repeat(20)}</span>
			{#if eng.total === 0}
				<LogsEmpty
					{loading}
					stopped={!project.running && !eng.ring.length}
					filtering={eng.filtering}
					filtered={!!(query || narrowed)}
					rangeLabel={rangeActive ? rangeText : null}
					onreset={resetFilters}
				/>
			{/if}
			<div class="spacer" style="height:{vp.contentH}px">
				<div
					class="rows"
					style="transform:translateY({vp.first < eng.view.length ? vp.topOf(vp.first) : 0}px)"
				>
					{#each vp.rows as { r, l } (l.seq)}
						<LogRow
							line={l}
							current={r === eng.cur}
							{matcher}
							wrap={vp.wrap}
							showTime={vp.showTime}
							showService={!only}
							onlink={openLink}
						/>
					{/each}
				</div>
			</div>
		</div>
		{#if vp.unseen > 0 && !vp.follow}
			<button class="pill" onclick={() => vp.toBottom()}>
				<ArrowDown size={12} />
				{vp.unseen > 9999 ? "9999+" : vp.unseen.toLocaleString()} new lines
			</button>
		{/if}
	</div>

	<LogsStatus
		total={eng.total}
		ofTotal={narrowed ? eng.ring.length : null}
		errors={eng.errors}
		warns={eng.warns}
		filtering={eng.filtering}
	/>
</div>

<style>
	.logs {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		min-width: 0;
	}
	.tools {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	.viewport {
		flex: 1;
		min-height: 0;
		overflow: auto;
		position: relative;
		font-family: var(--mono);
		font-size: var(--atlas-buffer-font-size, 12px);
		background: var(--ca-surface-sunken);
		outline: none;
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
		overscroll-behavior: contain;
	}
	.viewport.meta :global(.link) {
		cursor: pointer;
		text-decoration: underline;
	}
	.probe {
		position: absolute;
		visibility: hidden;
		white-space: pre;
		pointer-events: none;
	}
	.spacer {
		position: relative;
	}
	.rows {
		position: absolute;
		inset: 0 0 auto 0;
	}
	.body {
		position: relative;
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	.pill {
		position: absolute;
		bottom: 14px;
		right: 22px;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 5px 10px;
		border: 0;
		border-radius: 999px;
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
		font: inherit;
		font-family: -apple-system, sans-serif;
		font-size: 11px;
		cursor: pointer;
		box-shadow: 0 2px 10px rgb(0 0 0 / 0.25);
	}
</style>
