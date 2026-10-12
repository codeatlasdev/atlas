<script lang="ts">
	import { onDestroy, untrack } from "svelte";
	import { Chip, EmptyState, Kbd, Spinner, Switch, toast } from "@codeatlasdev/ui";
	import {
		ArrowDown,
		ChevronDown,
		ChevronUp,
		Clock,
		Copy,
		Sparkles,
		TriangleAlert,
		CircleAlert,
		WrapText,
		ScrollText,
		X,
	} from "@lucide/svelte";
	import { api, type ProjectView } from "#lib/api.ts";
	import { store } from "#lib/stores/projects.svelte.ts";
	import { activity } from "#lib/panel/activity.svelte.ts";
	import { ws } from "#lib/stores/workspace.svelte.ts";
	import { openFile } from "#lib/stores/files.svelte.ts";
	import { openPreview } from "#lib/stores/browser.svelte.ts";
	import { css, parseAnsi, type Run } from "#lib/ansi.ts";
	import {
		ALL_TIME,
		LAST_PRESETS,
		NO_WINDOW,
		Refilter,
		Ring,
		Search,
		View,
		compileQuery,
		findLinks,
		format,
		hms,
		hue,
		jumpLevel,
		matchesIn,
		parseCustomRange,
		rangeLabel,
		resolvePath,
		resolveWindow,
		rowAtTime,
		slideCutoff,
		windowHides,
		wrappedRows,
		type Filter,
		type TimeRange,
		type TimeWindow,
		type LinkSpan,
		type LogLine,
		type Level,
	} from "#lib/logs.ts";

	let { project, worktreeId }: { project: ProjectView; worktreeId?: string } = $props();

	const ROW = 18;
	const OVERSCAN = 12;
	/** Lines handed to the clipboard by "Copy for AI". */
	const AI_LINES = 300;
	/** Parsed runs kept for re-renders while scrolling. */
	const RUN_CACHE = 4000;

	// ───────── controls ─────────
	let only = $state<string | null>(store.focusedService);
	let minLevel = $state<Level>("info");
	let query = $state("");
	let useRegex = $state(false);
	let matchCase = $state(false);
	let hideNonMatching = $state(false);
	let showTime = $state(true);
	let wrap = $state(false);
	let follow = $state(true);

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

	// ───────── incremental view ─────────
	// Plain (non-reactive) engine state; `rev` is the only signal the template tracks.
	const e: {
		ring: Ring<LogLine>;
		view: View;
		consumed: number;
		refilter: Refilter | null;
		search: Search | null;
	} = {
		ring: store.logs(untrack(() => project.id)),
		view: new View(),
		consumed: 0,
		refilter: null,
		search: null,
	};
	e.consumed = e.ring.base + e.ring.length;
	let searchTimer = 0;
	let rev = $state(0);
	let total = $state(0);
	let errors = $state(0);
	let warns = $state(0);
	let matchCount = $state(0);
	let searching = $state(false);
	let counts = $state<Record<string, number>>({});

	/** Run `fn` in an idle slice; falls back to a timeout where `requestIdleCallback` is missing. */
	function idle(fn: () => void): number {
		return window.requestIdleCallback
			? window.requestIdleCallback(fn, { timeout: 100 })
			: (window as Window).setTimeout(fn, 8);
	}
	function cancelIdle(id: number) {
		if (window.cancelIdleCallback) window.cancelIdleCallback(id);
		else clearTimeout(id);
	}

	function publish() {
		total = e.view.length;
		errors = e.view.errors;
		warns = e.view.warns;
		rev++;
	}

	/** Per-service line counts for the chips, over the whole ring but only for new lines. */
	let countedUpTo = e.ring.base;
	function recount(reset: boolean) {
		if (reset) {
			counts = {};
			countedUpTo = e.ring.base;
		}
		const next = { ...counts };
		for (let i = Math.max(countedUpTo, e.ring.base) - e.ring.base; i < e.ring.length; i++) {
			const l = e.ring.items[i];
			if (l.mark === undefined && l.level === "error") next[l.name] = (next[l.name] ?? 0) + 1;
		}
		countedUpTo = e.ring.base + e.ring.length;
		counts = next;
	}

	/** Start (or restart) a chunked rebuild of the view for the current filter. */
	function rebuild() {
		if (refilterTimer) cancelIdle(refilterTimer);
		if (searchTimer) cancelIdle(searchTimer);
		searchTimer = 0;
		// The old search indexes the old view: drop it until the rebuild finishes.
		e.search = null;
		matchCount = 0;
		searching = false;
		cur = -1;
		e.view = new View();
		e.refilter = new Refilter(withClock(filter), e.ring);
		e.consumed = e.ring.base + e.ring.length;
		pumpRefilter();
	}
	let refilterTimer = 0;
	function pumpRefilter() {
		refilterTimer = 0;
		if (!e.refilter) return;
		const done = e.refilter.step(8);
		e.view = e.refilter.view;
		publish();
		if (done) {
			e.refilter = null;
			e.consumed = e.ring.base + e.ring.length;
			restartSearch();
		} else refilterTimer = idle(pumpRefilter);
	}

	function restartSearch() {
		if (searchTimer) cancelIdle(searchTimer);
		searchTimer = 0;
		e.search = matcher ? new Search(matcher, e.view, e.ring) : null;
		matchCount = 0;
		cur = -1;
		if (e.search) {
			searching = true;
			pumpSearch();
		} else searching = false;
	}
	function pumpSearch() {
		searchTimer = 0;
		if (!e.search) return;
		const done = e.search.scan(5);
		matchCount = e.search.rows.length;
		searching = !done;
		if (!done) searchTimer = idle(pumpSearch);
		else if (cur < 0 && e.search.rows.length && pendingJump) {
			pendingJump = false;
			step(1);
		}
	}

	/** Filter changes (service, level, query mode, time range) → full, time-sliced rebuild. */
	$effect(() => {
		void filter;
		untrack(rebuild);
	});

	// ───────── time range ─────────
	/** The filter with the sliding cutoff of this very moment (applied to lines as they are tested). */
	function withClock(f: Filter): Filter {
		return { ...f, cutoff: slideCutoff(f.win, Date.now()) };
	}

	/**
	 * Re-resolve the range against the ring. A moved boundary (a restart arrived, the range or the
	 * service changed, a new day for a custom range) changes `win`, which rebuilds the view once.
	 */
	function resolveRange() {
		const next = resolveWindow(range, e.ring, only, Date.now());
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

	/**
	 * A sliding window ages lines out by itself: once a second, drop what fell below the cutoff from
	 * the front of the view (O(dropped), no rescan). A straggler hiding behind a newer line rebuilds.
	 */
	function slideNow() {
		if (!win.lastMs || e.refilter) return;
		const dropped = e.view.slide(slideCutoff(win, Date.now()));
		if (dropped === "rebuild") return rebuild();
		if (!dropped) return;
		if (e.search) e.search.shift(dropped);
		if (cur >= 0) cur = Math.max(-1, cur - dropped);
		matchCount = e.search?.rows.length ?? 0;
		publish();
	}
	$effect(() => {
		if (!win.lastMs) return;
		const t = setInterval(slideNow, 1000);
		return () => clearInterval(t);
	});

	/** New lines → append to the view and refresh the counters; a replaced ring (resync) → rebuild. */
	$effect(() => {
		void live?.logRev;
		untrack(() => {
			const fresh = store.logs(project.id);
			if (fresh !== e.ring) {
				e.ring = fresh;
				recount(true);
				rebuild();
				return;
			}
			if (e.refilter) return; // the running rebuild reads to the ring's end
			const end = e.ring.base + e.ring.length;
			if (end === e.consumed) return;
			// The ring may have trimmed since `consumed`: drop view entries it no longer holds.
			const dropped = e.view.trim(e.ring.base);
			if (dropped && e.search) e.search.shift(dropped);
			if (dropped && cur >= 0) cur = Math.max(-1, cur - dropped);
			const had = e.view.length;
			e.view.add(withClock(filter), e.ring, e.consumed);
			e.consumed = end;
			recount(false);
			publish();
			if (e.search && e.view.length > had && !searchTimer) {
				searching = true;
				pumpSearch();
			}
			matchCount = e.search?.rows.length ?? 0;
		});
	});

	// ───────── range chip ─────────
	let rangeOpen = $state(false);
	let customText = $state("");
	let customError = $state("");
	let rangeLabelText = $derived(rangeLabel(range));

	function pickRange(r: TimeRange) {
		range = r;
		customError = "";
		rangeOpen = false;
	}
	function applyCustom() {
		const res = parseCustomRange(customText);
		if ("error" in res) customError = res.error;
		else {
			customText = "";
			pickRange(res.range);
		}
	}
	function onrangekey(ev: KeyboardEvent) {
		ev.stopPropagation();
		if (ev.key === "Enter") applyCustom();
		else if (ev.key === "Escape") rangeOpen = false;
		else customError = "";
	}

	onDestroy(() => {
		if (refilterTimer) cancelIdle(refilterTimer);
		if (searchTimer) cancelIdle(searchTimer);
	});

	// ───────── virtualization ─────────
	let viewport: HTMLDivElement | undefined = $state();
	let scrollTop = $state(0);
	let height = $state(400);
	let width = $state(800);
	let charW = $state(7.2);
	/** Offsets of each row's top in wrap mode (`offsets[r]`, length = rows + 1); fixed mode is `r * ROW`. */
	let offsets: number[] = [0];
	let offsetsFor = { rev: -1, cols: 0, wrap: false };

	let cols = $derived(
		Math.max(20, Math.floor((width - 32 - (showTime ? 70 : 0) - (only ? 0 : 102)) / charW)),
	);

	function lineAt(r: number): LogLine | undefined {
		return e.ring.items[e.view.idx[r] - e.ring.base];
	}

	/** Rebuild `offsets` when anything that changes row heights changed (O(rows), only in wrap mode). */
	function layout(): number {
		if (!wrap) return e.view.length * ROW;
		const key = { rev, cols, wrap };
		if (offsetsFor.rev !== key.rev || offsetsFor.cols !== key.cols || offsetsFor.wrap !== key.wrap) {
			const n = e.view.length;
			const o = new Array<number>(n + 1);
			let y = 0;
			for (let r = 0; r < n; r++) {
				o[r] = y;
				const l = lineAt(r);
				y += ROW * (l ? wrappedRows(l.line.length, cols) : 1);
			}
			o[n] = y;
			offsets = o;
			offsetsFor = key;
		}
		return offsets[e.view.length];
	}

	/** First row whose bottom is below `y`. */
	function rowAt(y: number): number {
		if (!wrap) return Math.floor(y / ROW);
		let lo = 0;
		let hi = e.view.length;
		while (lo < hi) {
			const mid = (lo + hi) >> 1;
			if (offsets[mid + 1] <= y) lo = mid + 1;
			else hi = mid;
		}
		return lo;
	}
	const topOf = (r: number) => (wrap ? offsets[r] : r * ROW);

	let contentH = $derived((void rev, void cols, void wrap, void showTime, layout()));
	let first = $derived(Math.max(0, rowAt(scrollTop) - OVERSCAN));
	let last = $derived(Math.min(total, rowAt(scrollTop + height) + 1 + OVERSCAN));
	let rows = $derived.by(() => {
		void rev;
		const out: { r: number; l: LogLine }[] = [];
		for (let r = first; r < last; r++) {
			const l = lineAt(r);
			if (l) out.push({ r, l });
		}
		return out;
	});

	let unseen = $state(0);
	let pinned = false;

	/** Stay on the newest line while following; otherwise count what arrived below. */
	let lastTotal = 0;
	$effect(() => {
		void rev;
		void contentH;
		untrack(() => {
			const added = Math.max(0, total - lastTotal);
			lastTotal = total;
			if (follow && viewport) {
				pinned = true;
				queueMicrotask(() => {
					if (viewport) viewport.scrollTop = viewport.scrollHeight;
					pinned = false;
				});
				unseen = 0;
			} else unseen += added;
		});
	});

	/** Only the user scrolling up stops following: a trim or a burst that moves the
	 * content under a pinned viewport must not (its scroll event lands after the pin). */
	let lastScroll = 0;
	function onscroll() {
		if (!viewport || pinned) return;
		const st = viewport.scrollTop;
		const up = st < lastScroll - 1;
		lastScroll = st;
		scrollTop = st;
		const atBottom = viewport.scrollHeight - st - viewport.clientHeight < ROW * 2;
		if (atBottom) {
			follow = true;
			unseen = 0;
		} else if (up && follow) follow = false;
		else if (follow) viewport.scrollTop = viewport.scrollHeight;
	}

	function toBottom() {
		follow = true;
		unseen = 0;
		if (viewport) viewport.scrollTop = viewport.scrollHeight;
	}

	function scrollToRow(r: number, align: "nearest" | "center" = "center") {
		if (!viewport) return;
		follow = false;
		layout();
		const top = topOf(r);
		const h = wrap && offsets[r + 1] !== undefined ? offsets[r + 1] - top : ROW;
		if (align === "nearest") {
			if (top < viewport.scrollTop) viewport.scrollTop = top;
			else if (top + h > viewport.scrollTop + height) viewport.scrollTop = top + h - height;
		} else viewport.scrollTop = Math.max(0, top - (height - h) / 2);
		scrollTop = viewport.scrollTop;
	}

	// "Open logs at that moment" from the Activity drawer: clear a time range that would hide the
	// moment (with a toast), narrow to the service, then center the first line at/after the time once
	// the (time-sliced) rebuild has caught up.
	$effect(() => {
		const s = activity.seek;
		if (!s || s.project !== project.id) return;
		void rev;
		untrack(() => {
			if (range.kind !== "all" && windowHides(win, e.ring, s.ts, Date.now())) {
				toast.info(`Cleared "${rangeLabel(range)}" to show that moment`);
				range = ALL_TIME;
				return;
			}
			if (only !== s.service) {
				only = s.service;
				return;
			}
			if (e.refilter || !viewport) return;
			activity.seek = null;
			const r = rowAtTime(e.view, e.ring, s.ts);
			if (r >= 0) scrollToRow(r);
		});
	});

	// Measure the monospace advance once and whenever the font changes.
	let probe: HTMLSpanElement | undefined = $state();
	$effect(() => {
		void rev;
		if (probe) {
			const w = probe.getBoundingClientRect().width / 20;
			if (w > 0 && Math.abs(w - charW) > 0.01) charW = w;
		}
	});

	// ───────── search navigation ─────────
	/** Current match (view row), or -1. */
	let cur = $state(-1);
	let pendingJump = false;
	let curOrdinal = $derived(e.search && cur >= 0 ? (void matchCount, e.search.ordinal(cur)) : 0);

	function step(dir: 1 | -1) {
		if (!e.search) return;
		if (!e.search.rows.length) {
			// Matches may still be coming in while the scan runs.
			pendingJump = searching;
			return;
		}
		const from = cur >= 0 ? cur : dir === 1 ? -1 : e.view.length;
		const r = dir === 1 ? e.search.next(from) : e.search.prev(from);
		if (r < 0) return;
		cur = r;
		scrollToRow(r);
	}

	/** First ⏎ after typing selects the match nearest the bottom (newest) when following, like a terminal. */
	function onsearchkey(e: KeyboardEvent) {
		e.stopPropagation();
		if (e.key === "Enter") {
			e.preventDefault();
			step(e.shiftKey ? -1 : 1);
		} else if (e.key === "Escape") {
			e.preventDefault();
			if (query) query = "";
			else viewport?.focus();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			step(1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			step(-1);
		}
	}

	function level(dir: 1 | -1, min: Level) {
		const from = cur >= 0 ? cur : follow ? e.view.length : Math.max(0, rowAt(scrollTop));
		const r = jumpLevel(e.view, e.ring, from, dir, min);
		if (r < 0) return toast.info(`No ${min} lines`);
		cur = r;
		scrollToRow(r);
	}

	let searchInput: HTMLInputElement | undefined = $state();
	function focusSearch() {
		searchInput?.focus();
		searchInput?.select();
	}

	// ───────── keyboard on the log body ─────────
	function onkeydown(e: KeyboardEvent) {
		if (e.metaKey && !e.shiftKey && !e.altKey && !e.ctrlKey && e.key === "f") {
			e.preventDefault();
			e.stopPropagation();
			return focusSearch();
		}
		if (e.metaKey && !e.shiftKey && !e.altKey && e.key === "g") {
			e.preventDefault();
			return step(1);
		}
		if (e.metaKey && e.shiftKey && e.key.toLowerCase() === "g") {
			e.preventDefault();
			return step(-1);
		}
		if (e.target !== viewport || e.metaKey || e.ctrlKey || e.altKey || !viewport) return;
		const page = Math.max(ROW, height - ROW * 2);
		switch (e.key) {
			case "j":
			case "ArrowDown":
				viewport.scrollTop += ROW * (e.shiftKey ? 8 : 1);
				break;
			case "k":
			case "ArrowUp":
				viewport.scrollTop -= ROW * (e.shiftKey ? 8 : 1);
				break;
			case " ":
			case "PageDown":
				viewport.scrollTop += page;
				break;
			case "PageUp":
				viewport.scrollTop -= page;
				break;
			case "g":
			case "Home":
				follow = false;
				viewport.scrollTop = 0;
				break;
			case "G":
			case "End":
				toBottom();
				break;
			case "n":
				step(e.shiftKey ? -1 : 1);
				break;
			case "N":
				step(-1);
				break;
			case "e":
				level(e.shiftKey ? -1 : 1, "error");
				break;
			case "E":
				level(-1, "error");
				break;
			case "w":
				level(e.shiftKey ? -1 : 1, "warn");
				break;
			case "W":
				level(-1, "warn");
				break;
			case "/":
				focusSearch();
				break;
			case "f":
				follow = !follow;
				if (follow) toBottom();
				break;
			case "t":
				showTime = !showTime;
				break;
			case "Escape":
				if (query) query = "";
				else if (cur >= 0) cur = -1;
				break;
			default:
				return;
		}
		e.preventDefault();
		e.stopPropagation();
	}

	// ───────── rendering helpers ─────────
	/** Parsed runs of lines that carry escapes, kept for repeated renders. */
	const runCache = new Map<number, Run[]>();
	function runsOf(l: LogLine): Run[] {
		if (!l.raw) return [{ text: l.line, style: null }];
		let r = runCache.get(l.seq);
		if (!r) {
			r = parseAnsi(l.raw);
			if (runCache.size >= RUN_CACHE) runCache.delete(runCache.keys().next().value!);
			runCache.set(l.seq, r);
		}
		return r;
	}

	interface Seg {
		text: string;
		css: string;
		hit: 0 | 1 | 2;
		link?: LinkSpan | { kind: "osc"; target: string };
	}

	/** Runs split at search hits and links (both may cross run boundaries). */
	function segments(l: LogLine, r: number): Seg[] {
		const text = l.line;
		const hits = matcher ? matchesIn(matcher, text) : [];
		const links = findLinks(text);
		const runs = runsOf(l);
		if (!hits.length && !links.length && runs.length === 1 && !runs[0].style?.link) {
			return [{ text, css: css(runs[0].style), hit: 0 }];
		}
		const strong = r === cur;
		const out: Seg[] = [];
		let pos = 0;
		let h = 0;
		let k = 0;
		for (const run of runs) {
			const style = css(run.style);
			const osc = run.style?.link;
			let i = 0;
			while (i < run.text.length) {
				const abs = pos + i;
				while (h < hits.length && hits[h][1] <= abs) h++;
				while (k < links.length && links[k].end <= abs) k++;
				const inHit = h < hits.length && hits[h][0] <= abs;
				const inLink = k < links.length && links[k].start <= abs;
				let edge = Infinity;
				if (h < hits.length) edge = Math.min(edge, inHit ? hits[h][1] : hits[h][0]);
				if (k < links.length) edge = Math.min(edge, inLink ? links[k].end : links[k].start);
				const end = Math.min(run.text.length, edge - pos);
				out.push({
					text: run.text.slice(i, end),
					css: style,
					hit: inHit ? (strong ? 2 : 1) : 0,
					link: osc ? { kind: "osc", target: osc } : inLink ? links[k] : undefined,
				});
				i = end;
			}
			pos += run.text.length;
		}
		return out;
	}

	// ───────── links ─────────
	const wtId = $derived(worktreeId ?? ws.selected?.id);
	/** File links resolve against the tab's worktree checkout (a linked worktree has its own path). */
	const root = $derived(ws.worktree(wtId)?.path ?? project.root);

	async function openLink(e: MouseEvent, seg: Seg, l: LogLine) {
		const link = seg.link;
		if (!link || !e.metaKey) return;
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
	const mods = (e: KeyboardEvent) => (metaDown = e.metaKey);

	// ───────── copy ─────────
	/** The filtered view as lines (cap = last `max`), for copy and "Copy for AI". */
	function viewLines(max = Infinity): LogLine[] {
		const out: LogLine[] = [];
		for (let r = Math.max(0, e.view.length - max); r < e.view.length; r++) {
			const l = lineAt(r);
			if (l) out.push(l);
		}
		return out;
	}

	async function copyVisible() {
		const lines = viewLines();
		await navigator.clipboard.writeText(format(lines, showTime));
		toast.success(`Copied ${lines.length.toLocaleString()} lines`);
	}

	/** Last lines of the filtered view with a header that tells a model what it is looking at. */
	async function copyForAi() {
		const lines = viewLines(AI_LINES);
		const scope = only ?? "all services";
		const head = `Logs of ${project.name} (${scope}${minLevel !== "info" ? `, ${minLevel}+` : ""}${query ? `, matching "${query}"` : ""}), last ${lines.length} lines:`;
		await navigator.clipboard.writeText(`${head}\n\`\`\`\n${format(lines, showTime)}\n\`\`\``);
		toast.success(`Copied ${lines.length} lines for AI`);
	}

	/** Copying a selection keeps the plain `text`, not the decorated DOM (timestamps, labels). */
	function oncopy(e: ClipboardEvent) {
		const sel = getSelection();
		if (!sel || sel.isCollapsed || !e.clipboardData) return;
		if (!viewport?.contains(sel.anchorNode)) return;
		// The timestamp and service label are `user-select: none`; flex rows still emit blank lines between rows.
		e.clipboardData.setData("text/plain", sel.toString().replace(/\n{2,}/g, "\n"));
		e.preventDefault();
	}

	function setOnly(n: string | null) {
		only = n;
	}

	let loading = $derived(project.running && !live?.logsLoaded);
	let filtering = $derived(!!e.refilter || searching);
</script>

<svelte:window
	onkeydown={mods}
	onkeyup={mods}
	onblur={() => (metaDown = false)}
	onpointerdown={(ev) => {
		if (rangeOpen && !(ev.target as Element).closest?.(".rangewrap")) rangeOpen = false;
	}}
/>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="logs" {onkeydown} {oncopy}>
	<div class="bar">
		<div class="chips">
			<Chip size="sm" label="All" selected={!only} onclick={() => setOnly(null)} />
			{#each names as n (n)}
				<span class="svcchip" style="--h:{hue(n)}">
					<Chip
						size="sm"
						label={counts[n] ? `${n} · ${counts[n] > 999 ? "999+" : counts[n]}` : n}
						variant={counts[n] ? "error" : "default"}
						selected={only === n}
						onclick={() => setOnly(only === n ? null : n)}
					/>
				</span>
			{/each}
		</div>
		<div class="right">
			<div class="levels">
				{#each ["info", "warn", "error"] as const as lv (lv)}
					<Chip
						size="sm"
						variant={lv === "error" ? "error" : lv === "warn" ? "warning" : "default"}
						label={lv === "info" ? "all levels" : `${lv}+`}
						selected={minLevel === lv}
						onclick={() => (minLevel = lv)}
					/>
				{/each}
			</div>
		</div>
	</div>

	<div class="tools">
		<div class="search" class:bad={badRegex} role="search">
			<input
				bind:this={searchInput}
				bind:value={query}
				placeholder={hideNonMatching ? "Filter" : "Find"}
				aria-label="Search logs"
				spellcheck="false"
				autocomplete="off"
				onkeydown={onsearchkey}
			/>
			<button
				class="toggle"
				class:on={matchCase}
				title="Match case"
				aria-label="Match case"
				aria-pressed={matchCase}
				onclick={() => (matchCase = !matchCase)}>Aa</button
			>
			<button
				class="toggle"
				class:on={useRegex}
				title="Regular expression"
				aria-label="Regular expression"
				aria-pressed={useRegex}
				onclick={() => (useRegex = !useRegex)}>.*</button
			>
			<button
				class="toggle"
				class:on={hideNonMatching}
				title={hideNonMatching ? "Filtering: hide lines that do not match" : "Finding: highlight matches"}
				aria-label="Hide non-matching lines"
				aria-pressed={hideNonMatching}
				onclick={() => (hideNonMatching = !hideNonMatching)}>Filter</button
			>
			<span class="count" aria-live="polite">
				{#if badRegex}<span class="badtxt">invalid</span>{:else if query}{#if matchCount}{curOrdinal
							? `${curOrdinal}/`
							: ""}{matchCount.toLocaleString()}{searching ? "+" : ""}{:else}{searching ? "…" : "0"}{/if}{/if}
			</span>
			<button class="icon" title="Previous match (⇧↵)" aria-label="Previous match" onclick={() => step(-1)}
				><ChevronUp size={14} /></button
			>
			<button class="icon" title="Next match (↵)" aria-label="Next match" onclick={() => step(1)}
				><ChevronDown size={14} /></button
			>
		</div>
		<div class="rangewrap">
			<div class="rangechip" class:on={range.kind !== "all"}>
				<button
					class="rangebtn"
					title="Time range"
					aria-label="Time range: {rangeLabelText}"
					aria-haspopup="dialog"
					aria-expanded={rangeOpen}
					onclick={() => (rangeOpen = !rangeOpen)}
				>
					<Clock size={13} />
					{#if range.kind !== "all"}<span>{rangeLabelText}</span>{/if}
				</button>
				{#if range.kind !== "all"}
					<button
						class="rangex"
						title="Clear time range"
						aria-label="Clear time range"
						onclick={() => pickRange(ALL_TIME)}><X size={11} /></button
					>
				{/if}
			</div>
			{#if rangeOpen}
				<div class="rangepop" role="dialog" aria-label="Time range">
					{#each LAST_PRESETS as m (m)}
						<button
							class="opt"
							class:sel={range.kind === "last" && range.minutes === m}
							onclick={() => pickRange({ kind: "last", minutes: m })}
							>{rangeLabel({ kind: "last", minutes: m })}</button
						>
					{/each}
					<button
						class="opt"
						class:sel={range.kind === "restart"}
						onclick={() => pickRange({ kind: "restart" })}
						>{only ? `Since ${only} restarted` : "Since last restart"}</button
					>
					<button class="opt" class:sel={range.kind === "start"} onclick={() => pickRange({ kind: "start" })}
						>Since stack start</button
					>
					<div class="custom">
						<input
							bind:value={customText}
							placeholder="14:05 or 14:05–14:20"
							aria-label="Custom time range (today)"
							aria-invalid={!!customError}
							spellcheck="false"
							autocomplete="off"
							onkeydown={onrangekey}
						/>
						<button class="toggle" onclick={applyCustom}>Apply</button>
					</div>
					{#if customError}<div class="rangeerr" role="alert">{customError}</div>{/if}
				</div>
			{/if}
		</div>
		<div class="jump">
			<button
				class="icon err"
				title="Previous error (E)"
				aria-label="Previous error"
				onclick={() => level(-1, "error")}
			>
				<CircleAlert size={13} />
				<ChevronUp size={11} />
			</button>
			<button
				class="icon err"
				title="Next error (e)"
				aria-label="Next error"
				onclick={() => level(1, "error")}
			>
				<CircleAlert size={13} />
				<ChevronDown size={11} />
			</button>
			<button
				class="icon warn"
				title="Previous warning (W)"
				aria-label="Previous warning"
				onclick={() => level(-1, "warn")}
			>
				<TriangleAlert size={13} />
				<ChevronUp size={11} />
			</button>
			<button
				class="icon warn"
				title="Next warning (w)"
				aria-label="Next warning"
				onclick={() => level(1, "warn")}
			>
				<TriangleAlert size={13} />
				<ChevronDown size={11} />
			</button>
		</div>
		<div class="right">
			<button
				class="icon"
				class:on={showTime}
				title="Timestamps (t)"
				aria-label="Timestamps"
				aria-pressed={showTime}
				onclick={() => (showTime = !showTime)}><Clock size={14} /></button
			>
			<button
				class="icon"
				class:on={wrap}
				title="Wrap long lines"
				aria-label="Wrap long lines"
				aria-pressed={wrap}
				onclick={() => (wrap = !wrap)}><WrapText size={14} /></button
			>
			<Switch label="Follow" bind:checked={follow} onchange={() => follow && toBottom()} />
			<button class="icon" onclick={copyVisible} title="Copy filtered lines" aria-label="Copy filtered lines"
				><Copy size={14} /></button
			>
			<button
				class="icon"
				onclick={copyForAi}
				title="Copy last {AI_LINES} lines for AI"
				aria-label="Copy for AI"><Sparkles size={14} /></button
			>
		</div>
	</div>

	<div class="body">
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="viewport"
			class:meta={metaDown}
			bind:this={viewport}
			bind:clientHeight={height}
			bind:clientWidth={width}
			{onscroll}
			tabindex="0"
			role="log"
			aria-label="Service logs"
			aria-live="off"
		>
			<span class="probe" bind:this={probe} aria-hidden="true">{"0".repeat(20)}</span>
			{#if total === 0}
				<div class="none">
					{#if loading}
						<Spinner size={20} label="Loading logs" />
					{:else if !project.running && !e.ring.length}
						<EmptyState
							icon={ScrollText}
							title="Stack is stopped"
							description="Logs stream here once it runs."
						/>
					{:else if filtering}
						<Spinner size={20} label="Filtering" />
					{:else if query || only || minLevel !== "info" || range.kind !== "all"}
						<EmptyState
							icon={ScrollText}
							title="No lines match"
							description={range.kind !== "all"
								? `Nothing in "${rangeLabelText}". Clear the range, the search or the filters.`
								: "Clear the search or the filters."}
						>
							{#snippet actions()}
								<Chip
									size="sm"
									label="Reset filters"
									onclick={() => {
										query = "";
										only = null;
										minLevel = "info";
										range = ALL_TIME;
									}}
								/>
							{/snippet}
						</EmptyState>
					{:else}
						<EmptyState icon={ScrollText} title="Waiting for output…" />
					{/if}
				</div>
			{/if}
			<div class="spacer" style="height:{contentH}px">
				<div class="rows" style="transform:translateY({first < e.view.length ? topOf(first) : 0}px)">
					{#each rows as { r, l } (l.seq)}
						{#if l.mark !== undefined}
							<div class="mark" style="--h:{hue(l.name)}">
								<span>{hms(l.ts)} · {l.mark}</span>
							</div>
						{:else}
							<div
								class="row lv-{l.level}"
								class:err={l.stream === "stderr"}
								class:wrap
								class:cur={r === cur}
								style={wrap ? "" : `height:${ROW}px`}
							>
								{#if showTime}<span class="ts">{hms(l.ts)}</span>{/if}
								{#if !only}<span class="svc" style="--h:{hue(l.name)}">{l.name}</span>{/if}
								<span class="txt"
									>{#each segments(l, r) as p, i (i)}{#if p.link}<span
												class="link"
												class:hit={p.hit === 1}
												class:cur={p.hit === 2}
												style={p.css}
												title={p.link.kind === "file"
													? `⌘-click to open ${p.link.target}${p.link.line ? `:${p.link.line}` : ""}`
													: `⌘-click to open ${p.link.target}`}
												role="link"
												tabindex="-1"
												onclick={(e) => openLink(e, p, l)}
												onkeydown={() => {}}>{p.text}</span
											>{:else if p.hit}<mark class:cur={p.hit === 2} style={p.css}>{p.text}</mark
											>{:else if p.css}<span style={p.css}>{p.text}</span>{:else}{p.text}{/if}{/each}</span
								>
							</div>
						{/if}
					{/each}
				</div>
			</div>
		</div>
		{#if unseen > 0 && !follow}
			<button class="pill" onclick={toBottom}>
				<ArrowDown size={12} />
				{unseen > 9999 ? "9999+" : unseen.toLocaleString()} new lines
			</button>
		{/if}
	</div>

	<div class="status">
		<span>
			{total.toLocaleString()} lines{only ||
			minLevel !== "info" ||
			range.kind !== "all" ||
			(hideNonMatching && matcher)
				? ` of ${e.ring.length.toLocaleString()}`
				: ""}
		</span>
		{#if errors}<span class="e">{errors.toLocaleString()} errors</span>{/if}
		{#if warns}<span class="w">{warns.toLocaleString()} warnings</span>{/if}
		{#if filtering}<span class="busy"><Spinner size={11} label="Working" /> filtering…</span>{/if}
		<span class="hint"
			><Kbd>⌘F</Kbd> find · <Kbd>e</Kbd>
			<Kbd>w</Kbd> next error / warn · <Kbd>⌘</Kbd>-click links ·
			<Kbd>⌘L</Kbd> logs</span
		>
	</div>
</div>

<style>
	.logs {
		display: flex;
		flex-direction: column;
		flex: 1;
		min-height: 0;
		min-width: 0;
	}
	.bar,
	.tools {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		border-bottom: 0.5px solid var(--ca-outline-subtle);
		flex-wrap: wrap;
	}
	.tools {
		padding: 6px 16px;
	}
	.chips,
	.levels,
	.jump {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
		align-items: center;
	}
	.jump {
		gap: 2px;
	}
	.svcchip :global(.ca-chip::before) {
		content: "";
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: oklch(0.68 0.14 var(--h));
		margin-right: 5px;
		flex-shrink: 0;
	}
	.right {
		margin-left: auto;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.search input {
		font: inherit;
		font-size: 12px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-bg-default));
		color: var(--ca-text-primary);
		width: 220px;
		user-select: text;
		-webkit-user-select: text;
		cursor: text;
	}
	.search.bad input {
		border-color: var(--ca-semantic-error);
		color: var(--ca-semantic-error);
	}
	.toggle {
		font: inherit;
		font-size: 11px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.toggle.on,
	.icon.on {
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.rangewrap {
		position: relative;
	}
	.rangechip {
		display: inline-flex;
		align-items: center;
		height: 26px;
		border-radius: 7px;
		border: 0.5px solid transparent;
		color: var(--ca-text-secondary);
	}
	.rangechip.on {
		border-color: var(--ca-outline-subtle);
		background: var(--ca-brand-fill);
		color: var(--ca-brand-contrast);
	}
	.rangebtn,
	.rangex {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 100%;
		border: 0;
		background: transparent;
		color: inherit;
		font: inherit;
		font-size: 11px;
		cursor: pointer;
	}
	.rangebtn {
		padding: 0 8px;
		border-radius: 7px;
	}
	.rangex {
		padding: 0 6px 0 0;
	}
	.rangechip:not(.on) .rangebtn:hover {
		background: var(--ca-fill-hover, rgb(128 128 128 / 0.15));
	}
	.rangepop {
		position: absolute;
		z-index: 20;
		top: calc(100% + 4px);
		left: 0;
		min-width: 200px;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding: 4px;
		border-radius: 9px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: var(--ca-surface-raised, var(--ca-surface, #1c1c1e));
		box-shadow: 0 6px 24px rgb(0 0 0 / 0.3);
	}
	.opt {
		text-align: left;
		font: inherit;
		font-size: 12px;
		padding: 5px 8px;
		border: 0;
		border-radius: 6px;
		background: transparent;
		color: var(--ca-text-primary, inherit);
		cursor: pointer;
	}
	.opt:hover {
		background: var(--ca-fill-hover, rgb(128 128 128 / 0.15));
	}
	.opt.sel {
		color: var(--ca-brand-fill);
		font-weight: 600;
	}
	.custom {
		display: flex;
		gap: 6px;
		padding: 6px 4px 2px;
		border-top: 0.5px solid var(--ca-outline-subtle);
		margin-top: 3px;
	}
	.custom input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 12px;
		padding: 3px 6px;
		border-radius: 5px;
		border: 0.5px solid var(--ca-outline-subtle);
		background: transparent;
		color: inherit;
	}
	.custom input[aria-invalid="true"] {
		border-color: var(--ca-error, #ff453a);
	}
	.rangeerr {
		padding: 2px 8px 4px;
		font-size: 11px;
		color: var(--ca-error, #ff453a);
	}
	.count {
		min-width: 44px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
	.icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 1px;
		min-width: 26px;
		height: 26px;
		border: 0;
		border-radius: 7px;
		background: transparent;
		color: var(--ca-text-secondary);
		cursor: pointer;
	}
	.icon:hover:not(.on) {
		background: var(--ca-action-hover);
	}
	.icon.err {
		color: var(--ca-semantic-error);
	}
	.icon.warn {
		color: var(--ca-semantic-warning);
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
	.row {
		line-height: 18px;
		padding: 0 16px;
		white-space: pre;
		display: flex;
		gap: 10px;
		font-variant-ligatures: none;
	}
	.row.wrap {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.row:hover {
		background: var(--ca-action-hover);
	}
	.row.cur {
		background: color-mix(in oklab, var(--ca-brand-fill) 14%, transparent);
	}
	.ts,
	.svc {
		user-select: none;
		-webkit-user-select: none;
	}
	.ts {
		color: var(--ca-text-disabled);
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
	}
	.svc {
		flex-shrink: 0;
		width: 92px;
		overflow: hidden;
		text-overflow: ellipsis;
		color: oklch(0.68 0.14 var(--h));
	}
	.txt {
		color: var(--ca-text-primary);
		min-width: 0;
	}
	.lv-warn .txt {
		color: var(--ca-semantic-warning);
	}
	.lv-error .txt {
		color: var(--ca-semantic-error);
	}
	mark,
	.link.hit {
		background: color-mix(in oklab, var(--ca-semantic-warning) 40%, transparent);
		color: inherit;
		border-radius: 2px;
	}
	mark.cur,
	.link.cur {
		background: var(--ca-semantic-warning);
		color: var(--ca-surface-base);
		border-radius: 2px;
	}
	.mark {
		display: flex;
		align-items: center;
		height: 18px;
		padding: 0 16px;
		gap: 10px;
		color: oklch(0.68 0.14 var(--h));
		font-size: 11px;
		user-select: none;
		-webkit-user-select: none;
	}
	.mark::before,
	.mark::after {
		content: "";
		flex: 1;
		border-top: 1px dashed color-mix(in oklab, currentColor 45%, transparent);
	}
	.none {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		color: var(--ca-text-secondary);
		z-index: 1;
		user-select: none;
		-webkit-user-select: none;
		cursor: default;
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
	.status {
		display: flex;
		gap: 12px;
		align-items: center;
		padding: 5px 16px;
		font-size: 11px;
		color: var(--ca-text-secondary);
		border-top: 0.5px solid var(--ca-outline-subtle);
	}
	.status .e,
	.badtxt {
		color: var(--ca-semantic-error);
	}
	.status .w {
		color: var(--ca-semantic-warning);
	}
	.busy {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.hint {
		margin-left: auto;
	}
</style>
