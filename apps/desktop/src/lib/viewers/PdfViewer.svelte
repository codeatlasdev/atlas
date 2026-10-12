<script lang="ts">
	import { onMount, tick, untrack } from "svelte";
	import {
		ZoomIn,
		ZoomOut,
		Maximize2,
		FileText,
		PanelLeft,
		Search,
		ChevronUp,
		ChevronDown,
		X,
		ExternalLink,
		FolderSearch,
		ChevronRight,
	} from "@lucide/svelte";
	import workerUrl from "pdfjs-dist/build/pdf.worker.mjs?url";
	import type { PDFDocumentLoadingTask, PDFDocumentProxy, PDFPageProxy, RenderTask } from "pdfjs-dist";
	import Tip from "#lib/components/Tip.svelte";
	import {
		PAGE_GAP,
		PAGE_PAD,
		TOOLBAR_IDLE_MS,
		anchorScroll,
		clampPdfZoom,
		currentPage,
		evictFar,
		findMatches,
		fitPageScale,
		fitWidthScale,
		layoutPages,
		parsePage,
		recallView,
		rememberView,
		stepMatch,
		stepZoom,
		visibleRange,
		wheelZoomFactor,
		type Match,
		type PageSize,
		type ViewState,
	} from "#lib/viewers/pdf.ts";

	let {
		blob,
		path,
		focused = false,
		onopen,
		onreveal,
	}: {
		blob: Blob;
		path: string;
		focused?: boolean;
		onopen: () => void;
		onreveal: () => void;
	} = $props();

	type Outline = { title: string; page: number | null; items: Outline[] };
	type Slot = {
		canvas?: HTMLCanvasElement;
		text?: HTMLDivElement;
		task?: RenderTask;
		/** Active text layer; replaced (and cancelled) whenever the page is re-laid out. */
		tl?: InstanceType<typeof import("pdfjs-dist").TextLayer>;
		/** Scale (css px per pdf pt × dpr) the canvas currently holds; 0 = empty. */
		done: number;
		busy: boolean;
		/** Scale of the render in flight, so identical requests are not started twice. */
		want?: number;
	};

	const THUMB_W = 96;
	const KEEP_BITMAPS = 6;
	const SETTLE_MS = 150;

	let scroller: HTMLDivElement | undefined = $state();
	let thumbRail: HTMLElement | undefined = $state();
	let doc: PDFDocumentProxy | undefined;
	let loading: PDFDocumentLoadingTask | undefined;
	let pdfjs: typeof import("pdfjs-dist") | undefined;
	let status = $state<"loading" | "ready" | "error">("loading");
	let errorText = $state("");
	let errorKind = $state<"password" | "invalid" | "other">("other");

	let pages = $state<PageSize[]>([]);
	let box = $state({ w: 0, h: 0 });
	/** "width"/"page" follow the box; a number is an explicit scale (1 = 100%). */
	let zoom = $state<number | "width" | "page">("width");
	let page = $state(0);
	/** Saved view to re-apply once the scroller has a real size (it is `display:none` until ready). */
	let pendingRestore = $state<ViewState | undefined>();
	let scrollTop = $state(0);
	/** Live pinch factor applied as a CSS transform until the gesture settles. */
	let pinch = $state({ k: 1, ox: 0, oy: 0 });

	let scale = $derived(
		zoom === "width" ? fitWidthScale(pages, box.w) : zoom === "page" ? fitPageScale(pages[page], box) : zoom,
	);
	let layout = $derived(layoutPages(pages, scale));
	let range = $derived(visibleRange(layout, scrollTop, box.h, 1));

	const slots = new Map<number, Slot>();
	const pageProxies = new Map<number, Promise<PDFPageProxy>>();
	const texts = new Map<number, string>();

	let sidebar = $state(false);
	let sideTab = $state<"thumbs" | "outline">("thumbs");
	let outline = $state<Outline[]>([]);
	let thumbs = new Map<number, HTMLCanvasElement>();

	let barVisible = $state(true);
	let barHover = false;
	let barTimer = 0;
	let editingPage = $state(false);
	let pageDraft = $state("");

	let findOpen = $state(false);
	let query = $state("");
	let findInput: HTMLInputElement | undefined = $state();
	let matches = $state<Match[]>([]);
	let cur = $state(-1);
	let searching = $state(false);
	let searchRun = 0;

	const stateKey = $derived(path);

	function showBar() {
		barVisible = true;
		clearTimeout(barTimer);
		barTimer = window.setTimeout(() => {
			if (!barHover && !findOpen && !editingPage) barVisible = false;
		}, TOOLBAR_IDLE_MS);
	}

	// ---- load ------------------------------------------------------------------------------

	onMount(() => {
		const ro = new ResizeObserver(([e]) => (box = { w: e.contentRect.width, h: e.contentRect.height }));
		ro.observe(scroller!);
		void open();
		showBar();
		return () => {
			ro.disconnect();
			clearTimeout(barTimer);
			clearTimeout(settleTimer);
			for (const s of slots.values()) s.task?.cancel();
			void loading?.destroy();
		};
	});

	async function open() {
		try {
			pdfjs = await import("pdfjs-dist");
			pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
			const base = `${location.origin}/pdfjs/`;
			const task = pdfjs.getDocument({
				data: new Uint8Array(await blob.arrayBuffer()),
				cMapUrl: `${base}cmaps/`,
				cMapPacked: true,
				standardFontDataUrl: `${base}standard_fonts/`,
				iccUrl: `${base}iccs/`,
				wasmUrl: `${base}wasm/`,
				// WebAssembly needs `wasm-unsafe-eval` in script-src; the JS fallbacks (JBIG2/JPX, PostScript
				// functions) keep the CSP minimal. Only qcms ICC conversion is lost (alternate colour space is used).
				useWasm: false,
				enableXfa: false,
			});
			doc = await (loading = task).promise;
			// Page sizes drive the layout, so all are needed; fetch them in parallel (the worker pipelines
			// getPage) instead of one round trip per page.
			const all = await Promise.all(
				Array.from({ length: doc.numPages }, async (_, i) => {
					const v = (await getPage(i)).getViewport({ scale: 1 });
					return { w: v.width, h: v.height };
				}),
			);
			pages = all;
			pendingRestore = recallView(stateKey);
			if (pendingRestore) {
				zoom = pendingRestore.zoom;
				page = pendingRestore.page;
			}
			status = "ready";
			void loadOutline();
		} catch (e) {
			const name = (e as { name?: string })?.name;
			errorKind =
				name === "PasswordException" ? "password" : name === "InvalidPDFException" ? "invalid" : "other";
			errorText = e instanceof Error ? e.message : String(e);
			status = "error";
		}
	}

	function getPage(i: number): Promise<PDFPageProxy> {
		let p = pageProxies.get(i);
		if (!p) pageProxies.set(i, (p = doc!.getPage(i + 1)));
		return p;
	}

	async function loadOutline() {
		const raw = await doc!.getOutline().catch(() => null);
		if (!raw?.length) return;
		const conv = async (items: NonNullable<typeof raw>): Promise<Outline[]> =>
			Promise.all(
				items.map(async (it) => {
					let pg: number | null = null;
					try {
						const dest = typeof it.dest === "string" ? await doc!.getDestination(it.dest) : it.dest;
						if (dest?.[0] && typeof dest[0] === "object") pg = await doc!.getPageIndex(dest[0]);
					} catch {
						/* unresolved destination: shown but not clickable */
					}
					return { title: it.title, page: pg, items: await conv(it.items ?? []) };
				}),
			);
		outline = await conv(raw);
	}

	function save() {
		if (status !== "ready" || !scroller || pendingRestore) return;
		const v: ViewState = { scrollTop: scroller.scrollTop, zoom, page };
		rememberView(stateKey, v);
	}

	// ---- page rendering (virtualized) -------------------------------------------------------

	const slotOf = (i: number): Slot => {
		let s = slots.get(i);
		if (!s) slots.set(i, (s = { done: 0, busy: false }));
		return s;
	};

	/** Renders page `i` at the current scale, crisp for the display; skips when already current. */
	async function paint(i: number) {
		const s = slotOf(i);
		const dpr = window.devicePixelRatio || 1;
		const want = scale * dpr;
		if (!s.canvas || (s.done === want && !s.busy) || (s.busy && s.want === want)) return;
		s.task?.cancel();
		s.busy = true;
		s.want = want;
		const pg = await getPage(i);
		// Scale changed while the page proxy resolved: a newer paint owns the slot.
		if (scale * dpr !== want || !s.canvas) {
			s.busy = false;
			return;
		}
		const vp = pg.getViewport({ scale: want });
		const canvas = s.canvas;
		const next = document.createElement("canvas");
		next.width = Math.floor(vp.width);
		next.height = Math.floor(vp.height);
		const task = pg.render({ canvasContext: next.getContext("2d")!, canvas: next, viewport: vp });
		s.task = task;
		try {
			await task.promise;
		} catch (e) {
			if ((e as { name?: string }).name !== "RenderingCancelledException") console.warn("pdf render", e);
			s.busy = false;
			return;
		}
		// Swap in the finished bitmap atomically so a re-render never flashes blank.
		canvas.width = next.width;
		canvas.height = next.height;
		canvas.getContext("2d")!.drawImage(next, 0, 0);
		s.done = want;
		s.busy = false;
		s.task = undefined;
		await paintText(i, scale);
		if (i === page) void paintThumb(i);
	}

	async function paintText(i: number, sc: number) {
		const s = slotOf(i);
		if (!s.text || !pdfjs) return;
		const pg = await getPage(i);
		// A newer paint may have started while the page proxy resolved; only the latest one renders.
		s.tl?.cancel();
		const host = s.text;
		host.replaceChildren();
		host.style.setProperty("--total-scale-factor", String(sc));
		const tl = (s.tl = new pdfjs.TextLayer({
			textContentSource: pg.streamTextContent(),
			container: host,
			viewport: pg.getViewport({ scale: sc }),
		}));
		await tl.render().catch(() => {});
		if (s.tl === tl) highlight(i);
	}

	function release(i: number) {
		const s = slots.get(i);
		if (!s) return;
		s.task?.cancel();
		s.task = undefined;
		s.busy = false;
		s.done = 0;
		if (s.canvas) {
			s.canvas.width = 0;
			s.canvas.height = 0;
		}
		s.tl?.cancel();
		s.tl = undefined;
		s.text?.replaceChildren();
	}

	// Paint what the window needs; release bitmaps far from the viewport.
	$effect(() => {
		if (status !== "ready" || !range) return;
		const [a, b] = range.near;
		const sc = scale;
		void sc;
		untrack(() => {
			const painted: number[] = [];
			for (const [i, s] of slots) if (s.done) painted.push(i);
			const [va, vb] = range.visible;
			// Visible pages first so they are never queued behind prefetch.
			const order: number[] = [];
			for (let i = va; i <= vb; i++) order.push(i);
			for (let i = a; i <= b; i++) if (i < va || i > vb) order.push(i);
			for (const i of order) void paint(i);
			const keep = Math.max(KEEP_BITMAPS, b - a + 1);
			for (const i of evictFar([...new Set([...painted, ...order])], page, keep)) release(i);
			for (const [i, s] of slots) if ((i < a || i > b) && (s.done || s.busy)) release(i);
		});
	});

	$effect(() => {
		if (!pendingRestore || box.h <= 0 || !scroller) return;
		const mem = pendingRestore;
		void layout.height;
		scroller.scrollTop = mem.scrollTop;
		scrollTop = scroller.scrollTop;
		// Only done once the browser accepted the offset (content tall enough to scroll to it).
		if (Math.abs(scroller.scrollTop - mem.scrollTop) < 2) pendingRestore = undefined;
	});

	// Remembered across tabs/reopens.
	$effect(() => {
		void zoom;
		void page;
		untrack(save);
	});

	// ---- scrolling / current page ------------------------------------------------------------

	function onscroll() {
		if (!scroller) return;
		scrollTop = scroller.scrollTop;
		page = currentPage(layout, scrollTop, box.h);
		save();
		showBar();
	}

	function goto(i: number, smooth = false) {
		if (!scroller || !pages.length) return;
		const n = Math.min(pages.length - 1, Math.max(0, i));
		scroller.scrollTo({
			top: Math.max(0, layout.tops[n] - PAGE_PAD / 2),
			behavior: smooth ? "smooth" : "instant",
		});
	}

	$effect(() => {
		if (!sidebar || !thumbRail) return;
		const el = thumbRail.querySelector<HTMLElement>(`[data-thumb="${page}"]`);
		el?.scrollIntoView({ block: "nearest" });
	});

	// ---- zoom --------------------------------------------------------------------------------

	let settleTimer = 0;

	/** Commit a new scale around `at` (scroller-relative), preserving the content point under it. */
	function setZoom(next: number | "width" | "page", at?: { x: number; y: number }) {
		if (!scroller) return;
		const from = scale;
		const target = typeof next === "number" ? clampPdfZoom(next) : next;
		const to =
			target === "width"
				? fitWidthScale(pages, box.w)
				: target === "page"
					? fitPageScale(pages[page], box)
					: target;
		// Keep the same place in the document: anchor on the viewport center unless a cursor is given.
		const a = at ?? { x: box.w / 2, y: box.h / 2 };
		const s = anchorScroll(pages, { x: scroller.scrollLeft, y: scroller.scrollTop }, a, from, to);
		zoom = target;
		void tick().then(() => {
			scroller!.scrollLeft = s.x;
			scroller!.scrollTop = s.y;
			scrollTop = scroller!.scrollTop;
		});
	}

	function onwheel(e: WheelEvent) {
		showBar();
		// Trackpad pinch arrives as ctrlKey+wheel; ⌘+wheel zooms like Preview. Plain wheel scrolls natively.
		if (!(e.ctrlKey || e.metaKey) || !scroller) return;
		e.preventDefault();
		const r = scroller.getBoundingClientRect();
		const at = { x: e.clientX - r.left, y: e.clientY - r.top };
		const k = wheelZoomFactor(e.deltaY, e.ctrlKey);
		// Live phase: scale the already-rendered layer with CSS around the cursor (no re-render).
		const target = clampPdfZoom(scale * pinch.k * k);
		pinch = { k: target / scale, ox: at.x + scroller.scrollLeft, oy: at.y + scroller.scrollTop };
		clearTimeout(settleTimer);
		settleTimer = window.setTimeout(() => settlePinch(at), SETTLE_MS);
	}

	/** End of a pinch: commit the zoom, keep the point under the cursor fixed, re-render crisp. */
	function settlePinch(at: { x: number; y: number }) {
		const from = scale;
		const to = clampPdfZoom(from * pinch.k);
		const s = anchorScroll(pages, { x: scroller!.scrollLeft, y: scroller!.scrollTop }, at, from, to);
		pinch = { k: 1, ox: 0, oy: 0 };
		zoom = to;
		void tick().then(() => {
			scroller!.scrollLeft = s.x;
			scroller!.scrollTop = s.y;
			scrollTop = scroller!.scrollTop;
		});
	}

	function toggleFit() {
		setZoom(zoom === "width" ? "page" : "width");
	}

	function ondblclick(e: MouseEvent) {
		// A double-click on a word selects it (native behavior); only blank areas toggle zoom.
		if ((e.target as HTMLElement).closest(".tl span")) return;
		const r = scroller!.getBoundingClientRect();
		const at = { x: e.clientX - r.left, y: e.clientY - r.top };
		setZoom(zoom === "width" ? 1 : "width", at);
	}

	// ---- keyboard ----------------------------------------------------------------------------

	function onkeydown(e: KeyboardEvent) {
		if (!focused || status !== "ready") return;
		const typing = (e.target as HTMLElement | null)?.closest("input,textarea");
		if (e.metaKey || e.ctrlKey) {
			const k = e.key;
			if (k === "=" || k === "+") (setZoom(stepZoom(scale, 1)), e.preventDefault());
			else if (k === "-") (setZoom(stepZoom(scale, -1)), e.preventDefault());
			else if (k === "0") (setZoom("width"), e.preventDefault());
			else if (k === "1") (setZoom(1), e.preventDefault());
			else if (k.toLowerCase() === "f") (openFind(), e.preventDefault());
			else if (k.toLowerCase() === "g") (nextMatch(e.shiftKey ? -1 : 1), e.preventDefault());
			return;
		}
		if (typing || e.altKey) return;
		const line = 56;
		const vh = box.h * 0.9;
		const go = (dy: number) => scroller!.scrollBy({ top: dy, behavior: "instant" });
		switch (e.key) {
			case "ArrowDown":
			case "j":
				go(line);
				break;
			case "ArrowUp":
			case "k":
				go(-line);
				break;
			case " ":
				go(e.shiftKey ? -vh : vh);
				break;
			case "PageDown":
				go(vh);
				break;
			case "PageUp":
				go(-vh);
				break;
			case "Home":
				goto(0);
				break;
			case "End":
				scroller!.scrollTo({ top: layout.height });
				break;
			case "Escape":
				if (!findOpen) return;
				closeFind();
				break;
			default:
				return;
		}
		e.preventDefault();
		showBar();
	}

	// ---- search ------------------------------------------------------------------------------

	async function openFind() {
		findOpen = true;
		barVisible = true;
		await tick();
		findInput?.focus();
		findInput?.select();
	}

	function closeFind() {
		findOpen = false;
		query = "";
		matches = [];
		cur = -1;
		for (const i of slots.keys()) highlight(i);
		scroller?.focus();
		showBar();
	}

	async function pageText(i: number): Promise<string> {
		let t = texts.get(i);
		if (t === undefined) {
			const tc = await (await getPage(i)).getTextContent();
			t = tc.items.map((it) => ("str" in it ? it.str : "")).join("");
			texts.set(i, t);
		}
		return t;
	}

	async function runSearch() {
		const run = ++searchRun;
		const q = query;
		if (!q.trim() || !doc) {
			matches = [];
			cur = -1;
			for (const i of slots.keys()) highlight(i);
			return;
		}
		searching = true;
		const found: Match[] = [];
		for (let i = 0; i < doc.numPages; i++) {
			const t = await pageText(i);
			if (run !== searchRun) return;
			found.push(...findMatches(t, q, i));
		}
		searching = false;
		matches = found;
		// Start at the first match on/after the current page.
		const first = found.findIndex((m) => m.page >= page);
		cur = found.length ? (first === -1 ? 0 : first) : -1;
		for (const i of slots.keys()) highlight(i);
		revealMatch();
	}

	let searchDebounce = 0;
	function onQuery() {
		clearTimeout(searchDebounce);
		searchDebounce = window.setTimeout(runSearch, 180);
	}

	function nextMatch(dir: 1 | -1) {
		if (!matches.length) return;
		cur = stepMatch(cur, matches.length, dir);
		for (const i of slots.keys()) highlight(i);
		revealMatch();
	}

	function revealMatch() {
		const m = matches[cur];
		if (!m) return;
		const top = layout.tops[m.page];
		const vis = scroller!.scrollTop;
		const inView = top + layout.sizes[m.page].h > vis && top < vis + box.h;
		if (!inView) goto(m.page);
		// The text layer for the page may not exist yet: highlight() runs again after it paints.
		void tick().then(() => {
			const el = scroller?.querySelector<HTMLElement>("mark.hit.cur");
			if (!el || !scroller) return;
			const r = el.getBoundingClientRect();
			const sr = scroller.getBoundingClientRect();
			if (r.top < sr.top + 60 || r.bottom > sr.bottom - 90)
				scroller.scrollBy({ top: r.top - sr.top - box.h / 3, behavior: "instant" });
		});
	}

	/** Wraps this page's matches in `<mark>` inside its text layer spans. */
	function highlight(i: number) {
		const host = slots.get(i)?.text;
		if (!host) return;
		for (const m of host.querySelectorAll("mark.hit")) m.replaceWith(...m.childNodes);
		host.normalize();
		const list = matches.map((m, idx) => ({ m, idx })).filter((x) => x.m.page === i);
		if (!list.length) return;
		// Text layer spans concatenate to the same string pageText() built (marked content aside).
		const spans = [
			...host.querySelectorAll<HTMLElement>(
				":scope > span:not(.markedContent), :scope > .markedContent > span",
			),
		];
		let off = 0;
		let li = 0;
		for (const sp of spans) {
			const len = sp.textContent?.length ?? 0;
			const lo = off;
			const hi = off + len;
			off = hi;
			while (li < list.length && list[li].m.end <= lo) li++;
			// Process matches overlapping [lo, hi) for this span, right to left so offsets hold.
			const here: { s: number; e: number; cur: boolean }[] = [];
			for (let k = li; k < list.length && list[k].m.start < hi; k++) {
				const { m, idx } = list[k];
				here.push({ s: Math.max(m.start, lo) - lo, e: Math.min(m.end, hi) - lo, cur: idx === cur });
			}
			if (!here.length || !sp.firstChild || sp.firstChild.nodeType !== Node.TEXT_NODE) continue;
			for (const h of here.reverse()) {
				const tn = sp.firstChild as Text;
				const range = document.createRange();
				range.setStart(tn, h.s);
				range.setEnd(tn, h.e);
				const mark = document.createElement("mark");
				mark.className = h.cur ? "hit cur" : "hit";
				range.surroundContents(mark);
				// surroundContents splits the text node; the head stays first for the next (earlier) match.
			}
		}
	}

	// ---- thumbnails --------------------------------------------------------------------------

	async function paintThumb(i: number) {
		const c = thumbs.get(i);
		if (!c || !pages[i]) return;
		const pg = await getPage(i);
		const dpr = window.devicePixelRatio || 1;
		const sc = (THUMB_W / pages[i].w) * dpr;
		const vp = pg.getViewport({ scale: sc });
		c.width = Math.floor(vp.width);
		c.height = Math.floor(vp.height);
		await pg.render({ canvasContext: c.getContext("2d")!, canvas: c, viewport: vp }).promise.catch(() => {});
	}

	/** Svelte action: queue a thumbnail render when its rail item scrolls into view. */
	function thumb(node: HTMLCanvasElement, i: number) {
		thumbs.set(i, node);
		const io = new IntersectionObserver(
			([e]) => {
				if (e.isIntersecting) {
					void paintThumb(i);
					io.disconnect();
				}
			},
			{ root: thumbRail, rootMargin: "200px" },
		);
		io.observe(node);
		return {
			destroy() {
				io.disconnect();
				thumbs.delete(i);
			},
		};
	}

	/** Svelte action: binds a page slot's canvas and text layer to the virtualized renderer. */
	function slot(node: HTMLElement, i: number) {
		const s = slotOf(i);
		s.canvas = node.querySelector("canvas")!;
		s.text = node.querySelector(".tl") as HTMLDivElement;
		s.done = 0;
		return {
			destroy() {
				release(i);
				slots.delete(i);
			},
		};
	}

	function commitPage() {
		editingPage = false;
		const n = parsePage(pageDraft, pages.length);
		if (n !== null) goto(n);
	}

	let pct = $derived(Math.round(scale * pinch.k * 100));
	let indices = $derived(
		range ? Array.from({ length: range.near[1] - range.near[0] + 1 }, (_, k) => range.near[0] + k) : [],
	);
</script>

<svelte:window {onkeydown} />

<div class="pdf" class:side={sidebar && status === "ready"}>
	{#if status === "loading"}
		<div class="state" aria-busy="true">
			<div class="spin"></div>
			Opening PDF…
		</div>
	{:else if status === "error"}
		<div class="state">
			<FileText size={28} />
			<h3>
				{errorKind === "password"
					? "This PDF is password-protected"
					: errorKind === "invalid"
						? "This PDF is damaged or not a PDF"
						: "Could not render this PDF"}
			</h3>
			<p>
				{errorKind === "other"
					? errorText
					: "Atlas can't preview it here. Your default app can still open it."}
			</p>
			<div class="actions">
				<button class="primary" onclick={onopen}>Open externally</button>
				<button onclick={onreveal}>Reveal in Finder</button>
			</div>
		</div>
	{/if}

	{#if status === "ready" && sidebar}
		<aside class="rail" bind:this={thumbRail} aria-label="Pages">
			{#if outline.length}
				<div class="tabs">
					<button class:on={sideTab === "thumbs"} onclick={() => (sideTab = "thumbs")}>Pages</button>
					<button class:on={sideTab === "outline"} onclick={() => (sideTab = "outline")}>Outline</button>
				</div>
			{/if}
			{#if sideTab === "outline" && outline.length}
				<div class="outline">
					{#snippet node(items: Outline[], depth: number)}
						{#each items as it}
							<button
								class="ol"
								style:padding-left="{8 + depth * 12}px"
								disabled={it.page === null}
								onclick={() => it.page !== null && goto(it.page, true)}
							>
								{#if it.items.length}<ChevronRight size={10} />{:else}<span class="dot"></span>{/if}
								<span class="t">{it.title}</span>
							</button>
							{#if it.items.length}{@render node(it.items, depth + 1)}{/if}
						{/each}
					{/snippet}
					{@render node(outline, 0)}
				</div>
			{:else}
				{#each pages as p, i (i)}
					<button
						class="thumb"
						class:cur={i === page}
						data-thumb={i}
						onclick={() => goto(i)}
						aria-label="Page {i + 1}"
					>
						<canvas use:thumb={i} style:width="{THUMB_W}px" style:aspect-ratio="{p.w} / {p.h}"></canvas>
						<span>{i + 1}</span>
					</button>
				{/each}
			{/if}
		</aside>
	{/if}

	<div class="main" class:hidden={status !== "ready"}>
		<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
		<div
			class="scroller selectable"
			bind:this={scroller}
			{onscroll}
			{onwheel}
			{ondblclick}
			onpointermove={showBar}
			tabindex="0"
			role="document"
			aria-label={path}
		>
			<div
				class="content"
				style:width="{layout.width + PAGE_PAD * 2}px"
				style:height="{layout.height}px"
				style:transform={pinch.k !== 1 ? `scale(${pinch.k})` : undefined}
				style:transform-origin="{pinch.ox}px {pinch.oy}px"
			>
				{#each indices as i (i)}
					{@const sz = layout.sizes[i]}
					<div
						class="page"
						use:slot={i}
						style:top="{layout.tops[i]}px"
						style:width="{sz.w}px"
						style:height="{sz.h}px"
						style:left="max({PAGE_PAD}px, calc((100% - {sz.w}px) / 2))"
						style:--total-scale-factor={scale}
					>
						<canvas></canvas>
						<div class="tl textLayer"></div>
					</div>
				{/each}
			</div>
		</div>

		{#if status === "ready"}
			<div
				class="bar"
				class:show={barVisible || findOpen}
				onpointerenter={() => ((barHover = true), (barVisible = true))}
				onpointerleave={() => ((barHover = false), showBar())}
				role="toolbar"
				aria-label="PDF controls"
				tabindex="-1"
			>
				{#if findOpen}
					<div class="find">
						<Search size={13} />
						<input
							bind:this={findInput}
							bind:value={query}
							oninput={onQuery}
							placeholder="Find in PDF"
							spellcheck="false"
							onkeydown={(e) => {
								if (e.key === "Enter") (e.preventDefault(), nextMatch(e.shiftKey ? -1 : 1));
								else if (e.key === "Escape") (e.preventDefault(), closeFind());
								e.stopPropagation();
							}}
						/>
						<span class="count mono">
							{#if searching}…{:else if query.trim()}{matches.length
									? `${cur + 1} / ${matches.length}`
									: "0"}{/if}
						</span>
						<button aria-label="Previous match" disabled={!matches.length} onclick={() => nextMatch(-1)}
							><ChevronUp size={14} /></button
						>
						<button aria-label="Next match" disabled={!matches.length} onclick={() => nextMatch(1)}
							><ChevronDown size={14} /></button
						>
						<button aria-label="Close find" onclick={closeFind}><X size={14} /></button>
					</div>
				{:else}
					<Tip label="Pages & outline" side="top">
						{#snippet child(props)}
							<button
								{...props}
								class:on={sidebar}
								aria-label="Toggle sidebar"
								onclick={() => (sidebar = !sidebar)}><PanelLeft size={14} /></button
							>
						{/snippet}
					</Tip>
					<span class="sep"></span>
					<div class="pageno mono">
						{#if editingPage}
							<!-- svelte-ignore a11y_autofocus -->
							<input
								autofocus
								size="3"
								bind:value={pageDraft}
								onblur={commitPage}
								onkeydown={(e) => {
									if (e.key === "Enter") e.currentTarget.blur();
									else if (e.key === "Escape") ((editingPage = false), e.stopPropagation());
									e.stopPropagation();
								}}
								onfocus={(e) => e.currentTarget.select()}
							/>
						{:else}
							<button
								class="num"
								title="Go to page"
								onclick={() => ((pageDraft = String(page + 1)), (editingPage = true))}>{page + 1}</button
							>
						{/if}
						<span class="of">/ {pages.length}</span>
					</div>
					<span class="sep"></span>
					<Tip label="Zoom out" command="" side="top">
						{#snippet child(props)}
							<button {...props} aria-label="Zoom out" onclick={() => setZoom(stepZoom(scale, -1))}
								><ZoomOut size={14} /></button
							>
						{/snippet}
					</Tip>
					<button class="pct mono" title="Fit width" onclick={() => setZoom("width")}>{pct}%</button>
					<Tip label="Zoom in" side="top">
						{#snippet child(props)}
							<button {...props} aria-label="Zoom in" onclick={() => setZoom(stepZoom(scale, 1))}
								><ZoomIn size={14} /></button
							>
						{/snippet}
					</Tip>
					<Tip label={zoom === "page" ? "Fit width" : "Fit page"} side="top">
						{#snippet child(props)}
							<button
								{...props}
								aria-label="Toggle fit"
								class:on={typeof zoom === "string"}
								onclick={toggleFit}><Maximize2 size={14} /></button
							>
						{/snippet}
					</Tip>
					<span class="sep"></span>
					<Tip label="Find  ⌘F" side="top">
						{#snippet child(props)}
							<button {...props} aria-label="Find" onclick={openFind}><Search size={14} /></button>
						{/snippet}
					</Tip>
					<Tip label="Open externally" side="top">
						{#snippet child(props)}
							<button {...props} aria-label="Open externally" onclick={onopen}
								><ExternalLink size={14} /></button
							>
						{/snippet}
					</Tip>
					<Tip label="Reveal in Finder" side="top">
						{#snippet child(props)}
							<button {...props} aria-label="Reveal in Finder" onclick={onreveal}
								><FolderSearch size={14} /></button
							>
						{/snippet}
					</Tip>
				{/if}
			</div>
		{/if}
	</div>
</div>

<style>
	.pdf {
		flex: 1;
		min-height: 0;
		min-width: 0;
		display: flex;
		position: relative;
		/* Backdrop the sheets float on: deep in dark themes, soft grey in light. */
		--pdf-bg: color-mix(in oklab, var(--ca-text-primary) 6%, var(--ca-surface-sunken));
		--pdf-shadow:
			0 1px 2px color-mix(in oklab, black 30%, transparent),
			0 6px 20px color-mix(in oklab, black 26%, transparent);
		background: var(--pdf-bg);
	}
	.main {
		flex: 1;
		min-width: 0;
		min-height: 0;
		position: relative;
		display: flex;
	}
	.main.hidden {
		display: none;
	}
	.scroller {
		flex: 1;
		min-width: 0;
		overflow: auto;
		position: relative;
		outline: none;
		overscroll-behavior: contain;
		scrollbar-width: thin;
	}
	.content {
		position: relative;
		min-width: 100%;
		/* Pinch scales this layer live; the crisp re-render lands after the gesture settles. */
		will-change: transform;
	}
	.page {
		position: absolute;
		background: white;
		border-radius: 2px;
		box-shadow: var(--pdf-shadow);
		overflow: hidden;
		contain: layout paint;
	}
	.page canvas {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		display: block;
	}
	/* pdf.js text layer (mirrors pdfjs-dist/web/pdf_viewer.css): transparent glyphs over the canvas for
	   selection/copy. `--min-font-size` is set by TextLayer; sizes derive from `--total-scale-factor`. */
	.tl {
		position: absolute;
		inset: 0;
		overflow: clip;
		line-height: 1;
		letter-spacing: normal;
		word-spacing: normal;
		text-size-adjust: none;
		forced-color-adjust: none;
		transform-origin: 0 0;
		caret-color: CanvasText;
		z-index: 1;
		--min-font-size: 1;
		--text-scale-factor: calc(var(--total-scale-factor) * var(--min-font-size));
		--min-font-size-inv: calc(1 / var(--min-font-size));
		--scale-round-x: 1px;
		--scale-round-y: 1px;
	}
	.tl :global(:is(span, br)) {
		color: transparent;
		position: absolute;
		white-space: pre;
		cursor: text;
		transform-origin: 0% 0%;
		user-select: text;
	}
	.tl > :global(:not(.markedContent)),
	.tl :global(.markedContent span:not(.markedContent)) {
		z-index: 1;
		--font-height: 0;
		font-size: calc(var(--text-scale-factor) * var(--font-height));
		--scale-x: 1;
		--rotate: 0deg;
		transform: rotate(var(--rotate)) scaleX(var(--scale-x)) scale(var(--min-font-size-inv));
	}
	.tl :global(.markedContent) {
		display: contents;
	}
	.tl :global(span)::selection {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 40%, transparent);
	}
	.tl :global(mark.hit) {
		background: color-mix(in oklab, #ffd60a 55%, transparent);
		color: transparent;
		border-radius: 2px;
	}
	.tl :global(mark.hit.cur) {
		background: color-mix(in oklab, #ff9f0a 75%, transparent);
		outline: 1.5px solid #ff9f0a;
	}

	/* Floating glass pill, bottom-center; fades with idle. */
	.bar {
		position: absolute;
		left: 50%;
		bottom: 16px;
		transform: translate(-50%, 8px);
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 4px 6px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 999px;
		background: color-mix(in oklab, var(--ca-surface-raised) 78%, transparent);
		backdrop-filter: blur(18px) saturate(1.6);
		-webkit-backdrop-filter: blur(18px) saturate(1.6);
		box-shadow: 0 8px 28px color-mix(in oklab, black 28%, transparent);
		color: var(--ca-text-secondary);
		font-size: 11.5px;
		opacity: 0;
		pointer-events: none;
		transition:
			opacity 200ms ease-out,
			transform 200ms ease-out;
		z-index: 5;
	}
	.bar.show {
		opacity: 1;
		pointer-events: auto;
		transform: translate(-50%, 0);
	}
	.bar button {
		display: inline-grid;
		place-items: center;
		min-width: 26px;
		height: 26px;
		padding: 0 4px;
		border: 0;
		border-radius: 999px;
		background: none;
		color: inherit;
		transition: background 120ms ease-out;
	}
	.bar button:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.bar button:disabled {
		opacity: 0.4;
	}
	.bar button.on {
		color: var(--space-accent, var(--ca-brand-main));
	}
	.sep {
		width: 0.5px;
		height: 14px;
		margin: 0 4px;
		background: var(--ca-outline-subtle);
	}
	.mono {
		font-family: var(--mono);
	}
	.pageno {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
		color: var(--ca-text-primary);
	}
	.pageno .num {
		min-width: 22px;
		font-family: var(--mono);
		font-size: 11.5px;
	}
	.pageno .of {
		color: var(--ca-text-secondary);
	}
	.pageno input,
	.find input {
		border: 0;
		outline: none;
		background: color-mix(in oklab, var(--ca-text-primary) 10%, transparent);
		color: var(--ca-text-primary);
		border-radius: 6px;
		font: inherit;
		font-family: var(--mono);
		text-align: center;
		height: 22px;
		width: 36px;
	}
	.pct {
		min-width: 46px !important;
		font-size: 11.5px;
	}
	.find {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 4px;
	}
	.find input {
		width: 200px;
		padding: 0 8px;
		text-align: left;
		font-family: inherit;
	}
	.count {
		min-width: 48px;
		text-align: center;
		font-size: 11px;
	}

	/* Sidebar: thumbnails / outline rail. */
	.rail {
		width: 132px;
		flex-shrink: 0;
		overflow-y: auto;
		padding: 10px 0;
		border-right: 0.5px solid var(--ca-outline-subtle);
		background: color-mix(in oklab, var(--ca-text-primary) 3%, transparent);
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		scrollbar-width: thin;
	}
	.pdf.side .rail:has(.outline) {
		align-items: stretch;
	}
	.thumb {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 5px;
		border: 0;
		border-radius: 8px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 10.5px;
	}
	.thumb canvas {
		background: white;
		border-radius: 2px;
		box-shadow: 0 1px 4px color-mix(in oklab, black 30%, transparent);
	}
	.thumb:hover {
		background: var(--ca-action-hover);
	}
	.thumb.cur {
		background: color-mix(in oklab, var(--space-accent, var(--ca-brand-main)) 18%, transparent);
		color: var(--ca-text-primary);
	}
	.thumb.cur canvas {
		outline: 1.5px solid var(--space-accent, var(--ca-brand-main));
	}
	.tabs {
		display: flex;
		gap: 2px;
		padding: 0 8px;
		align-self: stretch;
	}
	.tabs button {
		flex: 1;
		height: 22px;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 11px;
	}
	.tabs button.on {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.outline {
		display: flex;
		flex-direction: column;
		align-self: stretch;
		padding: 0 4px;
	}
	.ol {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 22px;
		border: 0;
		border-radius: 5px;
		background: none;
		color: var(--ca-text-secondary);
		font-size: 11px;
		text-align: left;
	}
	.ol:hover:not(:disabled) {
		background: var(--ca-action-hover);
		color: var(--ca-text-primary);
	}
	.ol .t {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ol .dot {
		width: 10px;
		flex-shrink: 0;
	}

	.state {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 40px 20px;
		font-size: 12.5px;
		color: var(--ca-text-secondary);
		text-align: center;
	}
	.state h3 {
		margin: 4px 0 0;
		font-size: 15px;
		font-weight: 650;
		color: var(--ca-text-primary);
	}
	.state p {
		margin: 0 0 8px;
		max-width: 420px;
	}
	.actions {
		display: flex;
		gap: 8px;
	}
	.actions button {
		height: 28px;
		padding: 0 12px;
		border: 0.5px solid var(--ca-outline-subtle);
		border-radius: 8px;
		background: none;
		color: var(--ca-text-primary);
		font-size: 12px;
	}
	.actions button:hover {
		background: var(--ca-action-hover);
	}
	.actions .primary {
		border-color: transparent;
		background: var(--space-accent, var(--ca-brand-main));
		color: white;
	}
	.spin {
		width: 18px;
		height: 18px;
		border: 2px solid var(--ca-outline-subtle);
		border-top-color: var(--space-accent, var(--ca-brand-main));
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.bar {
			transition: none;
		}
		.spin {
			animation: none;
		}
	}
</style>
