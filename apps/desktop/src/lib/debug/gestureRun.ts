/**
 * One gesture-bench run inside the page: start the probe, hand the trace to the engine (native replay, never OS
 * input), wait until the engine and the layers are at rest, and return a `Run` for `assess` (gestureProbe.ts).
 */
import { invoke } from "@tauri-apps/api/core";
import { createProbe, type Run } from "./gestureProbe.ts";
import type { ScrollSample, Trace } from "../shell/trace.ts";

/** Time of the last finger lift (`ended`/`cancelled`) in the trace. */
export function liftAt(events: ScrollSample[]): number {
	let t = 0;
	for (const e of events)
		if (e.momentum === "none" && (e.phase === "ended" || e.phase === "cancelled")) t = e.t;
	return t;
}

/** First sample after which the layer stays within 0.001 page of its final position. */
export function restAt(samples: { t: number; x: number }[]): number {
	const end = samples.at(-1)?.x ?? 0;
	for (let i = samples.length - 1; i >= 0; i--)
		if (Math.abs(samples[i]!.x - end) > 0.001) return samples[i + 1]?.t ?? samples.at(-1)!.t;
	return samples[0]?.t ?? 0;
}

/**
 * Time from the event that claimed the gesture (`claimT`, in the same clock as the samples) to the first frame in
 * which a layer is off its rest position. The trace's own lead-in (`mayBegin`, `began`, the 15 pt gate) is not the
 * app's latency, so it is not counted; a frame painted before the claim gives 0, never a negative.
 */
export function firstMove(samples: { t: number; x: number }[], claimT: number): number {
	const moved = (s: { x: number }) => Math.abs(s.x) > 0.001;
	// already off rest when the claim arrived (a gesture preempting a settle): no latency to speak of
	const before = [...samples].reverse().find((s) => s.t <= claimT);
	if (before && moved(before)) return 0;
	const f = samples.find((s) => s.t >= claimT && moved(s));
	return f ? f.t - claimT : Infinity;
}

/**
 * Time (ms in the trace) of the sample that opens the engine's gate (15 pt of horizontal travel, at least twice
 * the vertical): the event whose latency to the first moved frame the bench holds to one display frame. Mirrors
 * `MIN_DX` / `AXIS_RATIO` / `MAX_DY` in swipe_engine.rs. Null when the trace never claims.
 */
export function claimAt(events: ScrollSample[]): number | null {
	let dx = 0;
	let dy = 0;
	for (const e of events) {
		if (!e.precise || e.momentum !== "none" || (e.phase !== "changed" && e.phase !== "began")) continue;
		dx += e.dx;
		dy += e.dy;
		if (Math.abs(dy) > 10 && Math.abs(dy) * 2 >= Math.abs(dx)) return null;
		if (Math.abs(dx) >= 15 && Math.abs(dx) > 2 * Math.abs(dy)) return e.t;
	}
	return null;
}

export interface EngineFrame {
	t: number;
	seq: number;
	amount: number;
	stage: string;
}

/**
 * The origin page's position in page widths over a run, from the engine's frames (authoritative, and continuous
 * across commits: reading the DOM layer instead breaks when the selection swaps which element is the origin). Each
 * generation's amounts are relative to the page it started on, so a generation is offset by where the earlier ones
 * left the page: a commit lands at -1, a cancel back at 0.
 */
export function frameSamples(frames: EngineFrame[]): { t: number; x: number }[] {
	const out: { t: number; x: number }[] = [];
	let landed = 0;
	let seq = 0;
	let last = 0;
	for (const f of frames) {
		if (f.seq !== seq) {
			if (seq) landed += -Math.round(last);
			seq = f.seq;
		}
		last = f.amount;
		out.push({ t: f.t, x: landed - f.amount });
	}
	return out;
}

/**
 * Native views (terminal, browser) parked and restored by a gesture, counted from the `visible` flag of their
 * bounds calls: a hide after a show is one park, a show after a hide one restore. A view left hidden at the end
 * shows up as parked > restored.
 */
export class NativeCount {
	#visible = new Map<string, boolean>();
	#parked = 0;
	#restored = 0;
	see(cmd: string, args: { tab?: string; visible?: boolean } | undefined) {
		if (cmd !== "term_bounds" && cmd !== "browser_bounds" && cmd !== "pager") return;
		const key = `${cmd}:${args?.tab ?? ""}`;
		const was = this.#visible.get(key);
		const now = args?.visible !== false;
		if (was === now) return;
		this.#visible.set(key, now);
		if (was === undefined) return;
		if (now) this.#restored++;
		else this.#parked++;
	}
	result() {
		return { parked: this.#parked, restored: this.#restored };
	}
}

interface EndState {
	selected: string;
	layers: { id: string; role: string; transform: string }[];
	/** The native snapshot overlay is still up (read back from native). */
	overlay: boolean;
}

/** What is inconsistent in the page after a gesture: roles, selection, leftover transforms, a stuck overlay. */
export function endStateProblems(s: EndState): string[] {
	const out: string[] = [];
	const current = s.layers.filter((l) => l.role === "current");
	if (current.length !== 1) out.push(`${current.length} current layers`);
	else if (current[0]!.id !== s.selected)
		out.push(`current layer is ${current[0]!.id} but the selection is ${s.selected}`);
	for (const l of s.layers) if (l.transform) out.push(`layer ${l.id} keeps a transform at rest`);
	if (s.overlay) out.push("the native overlay is still up");
	return out;
}

export interface GestureRunOpts {
	trace: Trace;
	/** Zone under test: pages are spaces ("space") or the tabs of the focused group ("tab"). */
	zone?: "space" | "tab";
	/** Tabs in the group for the tab zone (default 5). */
	tabs?: number;
	/** The pager layer elements are looked up through the shell's active space id. */
	hz?: number;
}

interface Shell {
	activeId: string | null;
	spaceIds: string[];
}

/** The workspace store, as far as the tab-zone bench needs it. */
interface Ws {
	selected?: { id: string };
	focusedGroup(worktreeId: string): { id: string; order: string[]; activeTab: string | null };
	openTab(worktreeId: string, partial: { kind: string; label: string }, groupId?: string): { id: string };
	activate(tabId: string): void;
}

/**
 * Tab zone: make the selected worktree's focused group hold `count` tabs and put the middle one in front,
 * so a swipe has a neighbour both ways. Returns the group id (the zone's id) and the tab order.
 */
export function prepareTabs(ws: Ws, count: number) {
	const wid = ws.selected?.id;
	if (!wid) throw new Error("no worktree selected");
	const g = ws.focusedGroup(wid);
	while (g.order.length < count)
		ws.openTab(wid, { kind: "services", label: `bench ${g.order.length}` }, g.id);
	ws.activate(g.order[Math.floor(g.order.length / 2)]!);
	return { group: g.id, order: [...g.order] };
}

/** What a zone's "page" is: where to read the selection and the order, and the layers for the end-state check. */
interface PageModel {
	selected(): string;
	order(): string[];
	/** Every page layer with its role and leftover transform, for the end-state check. */
	layers(): { id: string; role: string; transform: string }[];
}

function spaceModel(shell: Shell): PageModel {
	const els = () => [...document.querySelectorAll<HTMLElement>(".space-layer")];
	return {
		selected: () => shell.activeId ?? "",
		order: () => [...shell.spaceIds],
		layers: () =>
			els().map((l) => ({
				id: l.dataset.space ?? "",
				role: l.dataset.role ?? "",
				transform: l.style.transform,
			})),
	};
}

function tabModel(ws: Ws, group: string): PageModel {
	const g = () => ws.focusedGroup(ws.selected!.id);
	const body = () => {
		const active = g().activeTab;
		return document.querySelector<HTMLElement>(`.pane[data-tab="${CSS.escape(active ?? "")}"]`)
			?.parentElement;
	};
	return {
		selected: () => g().activeTab ?? "",
		order: () => [...g().order],
		layers: () =>
			[...(body()?.querySelectorAll<HTMLElement>(":scope > .pane") ?? [])].map((p) => ({
				id: p.dataset.tab ?? "",
				// a tab pane is "current" when it is the active one and not hidden
				role: p.dataset.tab === g().activeTab ? "current" : "near",
				transform: p.style.transform,
			})),
	};
}

export async function runGesture(o: GestureRunOpts & { shell: Shell; ws?: Ws }): Promise<
	Run & {
		hz: number;
		reflowSites: Record<string, number>;
		gestures: number;
		engineCommits: [number, number][];
		problems: string[];
	}
> {
	const { shell, trace } = o;
	const zone = o.zone ?? "space";
	let group = "";
	if (zone === "tab") {
		if (!o.ws) throw new Error("the tab zone needs the workspace store");
		group = prepareTabs(o.ws, o.tabs ?? 5).group;
		// the group re-renders its tabs and the native zone follows layout: let both settle
		await new Promise<void>((r) => setTimeout(r, 400));
	}
	const m = zone === "tab" ? tabModel(o.ws!, group) : spaceModel(shell);
	// When the native stage draws the slide the DOM layer does not move; its offset is the engine's own frame
	// amount (the numbers the stage places the overlay with).
	const frames: EngineFrame[] = [];
	const { listen } = await import("@tauri-apps/api/event");
	let t0 = 0;
	const un = await listen<{ amount: number; zone: string; seq: number; stage: string }>(
		"swipe://frame",
		(e) => {
			if (e.payload.zone === zone)
				frames.push({
					t: performance.now() - t0,
					seq: e.payload.seq,
					amount: e.payload.amount,
					stage: e.payload.stage,
				});
		},
	);
	// Native park/restore: the pager parks natives exactly while it is `moving` (space.live), so its
	// `.moving` class on the pager root is the signal; a gesture that leaves it set parks them for good.
	const natives = new NativeCount();
	const pagerEl = document.querySelector(".pager");
	const mo = new MutationObserver(() =>
		natives.see("pager", { tab: "pager", visible: !pagerEl?.classList.contains("moving") }),
	);
	if (pagerEl) {
		natives.see("pager", { tab: "pager", visible: !pagerEl.classList.contains("moving") });
		mo.observe(pagerEl, { attributes: true, attributeFilter: ["class"] });
	}
	const start = m.selected();
	const order = m.order();
	const probe = createProbe({ selected: m.selected });
	probe.start();
	t0 = performance.now();
	let report: { first_seq: number; last_seq: number; commits: [number, number][] };
	try {
		report = await invoke("gesture_replay_samples", { events: trace.events });
		// Let Svelte apply the commit, the overlay's ack lift and the natives come back: 600 ms of quiet.
		const quiet = Promise.withResolvers<void>();
		setTimeout(quiet.resolve, 600);
		await quiet.promise;
	} finally {
		un();
		mo.disconnect();
	}
	const rec = probe.stop();
	const lift = liftAt(trace.events);
	const samples = frameSamples(frames);
	const claim = claimAt(trace.events);
	const state = await invoke<{ overlay: boolean }>("pager_state").catch(() => ({ overlay: false }));
	const problems = endStateProblems({
		selected: m.selected(),
		layers: m.layers(),
		overlay: state.overlay,
	});
	return {
		trace: trace.name,
		expect: trace.expect,
		start,
		end: m.selected(),
		order,
		committed: rec.committed,
		frames: rec.frames,
		stalls: rec.stalls,
		reflows: rec.reflows,
		reflowSites: rec.reflowSites,
		settleMs: Math.max(0, restAt(samples) - lift),
		samples,
		firstMoveMs: claim === null ? Infinity : firstMove(samples, claim),
		native: natives.result(),
		consistent: problems.length === 0,
		hz: o.hz ?? 60,
		gestures: report.last_seq - report.first_seq,
		engineCommits: report.commits,
		problems,
	};
}

/** Record `ms` of a page doing nothing: the host's own jitter (frame gaps and stalls), the floor timing rules sit on. */
export async function idleFloor(ms = 3000) {
	const probe = createProbe({ selected: () => "" });
	probe.start();
	const done = Promise.withResolvers<void>();
	setTimeout(done.resolve, ms);
	await done.promise;
	const r = probe.stop();
	return { frames: r.frames, stalls: r.stalls };
}
