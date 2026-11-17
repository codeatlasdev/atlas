/**
 * The standard trace suite: every gesture shape the engine has to get right, synthesized from the
 * trackpad physics in `trace.ts` with fixed seeds so the JSON fixtures in
 * `src/lib/shell/__fixtures__/traces/` are byte-stable (`bun scripts/gesture-bench.ts --write-fixtures`
 * regenerates them; `traceSuite.test.ts` fails when they drift). User recordings (`recorded-*.json`) live in
 * the same directory and run through the same assertions.
 *
 * `expect.steps` is what a person who made that motion on a trackpad means: a fast flick or a drag past
 * the middle switches by exactly one page; a short slow drag, a vertical scroll or noise does not.
 */
import { BENCH_LAYOUT as L, chain, synth, type ScrollSample, type Trace } from "./trace.ts";

const sidebar = L.sidebar;
const strip = L.strip;
const content = L.content;
const native = L.nativeView;

const t = (name: string, source: string, expect: Trace["expect"], events: ScrollSample[]): Trace => ({
	name,
	source: `synth:${source}`,
	expect,
	events,
});

/** Sidebar swipes: 260 px wide pointer zone, one page = the content width the pager moves. */
/** What each trace's pointer is over: the sidebar for space swipes, the tab group's top band for strip swipes,
 * the content for tab drags/history, nothing for a titlebar swipe. */
const AIM: Record<string, NonNullable<Trace["expect"]["aim"]>> = {
	"tab-flick-next": "strip",
	"tab-many-tabs": "strip",
	"tab-drag-past-half": "strip",
	"over-native-view": "history",
	"no-zone-outside": "outside",
	"editor-scroll-owned": "strip",
	"editor-scroll-back-owned": "strip",
	"editor-at-edge-hands-over": "strip",
};

export function suite(): Trace[] {
	return build().map((t) => ({ ...t, expect: { ...t.expect, aim: AIM[t.name] ?? "sidebar" } }));
}

function build(): Trace[] {
	const sp = (o: Partial<Parameters<typeof synth>[0]> & { travel: number; ms: number }) =>
		synth({ x: sidebar.x, y: sidebar.y, seed: 7, ...o });
	return [
		t(
			"flick-next",
			"fast flick left over the sidebar, strong momentum",
			{ zone: "space", steps: 1 },
			sp({ travel: -90, ms: 80, peakAt: 0.35, momentum: 1.4 }),
		),
		t(
			"flick-prev",
			"fast flick right",
			{ zone: "space", steps: -1 },
			sp({ travel: 90, ms: 80, peakAt: 0.35, momentum: 1.4 }),
		),
		t(
			"slow-drag-past-half",
			"slow deliberate drag past half a page (250 of 420 pt), lift with no momentum",
			{ zone: "space", steps: 1 },
			sp({ travel: -250, ms: 1100, momentum: 0 }),
		),
		t(
			"slow-drag-short-cancel",
			"slow short drag, lift: springs back",
			{ zone: "space", steps: 0 },
			sp({ travel: -22, ms: 700, momentum: 0 }),
		),
		t(
			"medium-swipe",
			"ordinary two-finger swipe (230 pt in 260 ms), small momentum",
			{ zone: "space", steps: 1 },
			sp({ travel: -230, ms: 260, momentum: 0.6 }),
		),
		t(
			"reversal-commit-back",
			"drag left, reverse past the start, lift: goes to the previous one",
			{ zone: "space", steps: -1 },
			sp({ travel: -40, ms: 160, momentum: 0 })
				.slice(0, -1)
				.concat(
					synth({ x: sidebar.x, y: sidebar.y, seed: 8, travel: 270, ms: 330, at: 200, momentum: 0.4 }).slice(
						2,
					),
				),
		),
		t(
			"reversal-cancel",
			"drag left, come back to the start, lift: stays",
			{ zone: "space", steps: 0 },
			chain([
				{ events: sp({ travel: -60, ms: 200, momentum: 0 }).slice(0, -1) },
				{ events: synth({ x: sidebar.x, y: sidebar.y, seed: 9, travel: 60, ms: 200, momentum: 0 }).slice(2) },
			]),
		),
		t(
			"two-quick-swipes",
			"two flicks 240 ms apart: two gestures, one page each, the second begins mid-settle",
			{ zone: "space", steps: 2, gestures: 2 },
			chain([
				{ events: sp({ travel: -90, ms: 80, peakAt: 0.35, momentum: 1.0, seed: 3 }) },
				{ events: sp({ travel: -90, ms: 80, peakAt: 0.35, momentum: 1.0, seed: 4 }), gap: 240 },
			]),
		),
		t(
			"momentum-tail-never-chains",
			"a big flick: its momentum tail must not start a second switch",
			{ zone: "space", steps: 1 },
			sp({ travel: -160, ms: 90, peakAt: 0.3, momentum: 2.0 }),
		),
		t(
			"vertical-scroll",
			"vertical scroll over the sidebar: never a swipe",
			{ zone: "space", steps: 0, untouched: true },
			synth({ x: sidebar.x, y: sidebar.y, seed: 5, travel: 0, ms: 400, dyRatio: 0, momentum: 0 }).map((e) =>
				e.phase === "changed" ? { ...e, dy: -14 + (e.t % 5) } : e,
			),
		),
		t(
			"diagonal-vertical-dominant",
			"diagonal, vertical dominates: scrolling wins",
			{ zone: "space", steps: 0, untouched: true },
			sp({ travel: -50, ms: 300, dyRatio: 2.6, momentum: 0 }),
		),
		t(
			"diagonal-horizontal-dominant",
			"diagonal, horizontal dominates, small vertical leak: still a swipe",
			{ zone: "space", steps: 1 },
			sp({ travel: -240, ms: 300, dyRatio: 0.18, momentum: 0.5 }),
		),
		t(
			"tiny-jitter",
			"a resting finger: ±1 px jitter must not move anything",
			{ zone: "space", steps: 0, untouched: true },
			sp({ travel: -3, ms: 600, noise: 1.4, momentum: 0 }),
		),
		t(
			"first-space-rubber",
			"swipe toward a non-existent previous space: only a rubber band, never a commit",
			{ zone: "space", steps: 0, edge: "first" },
			sp({ travel: 140, ms: 200, momentum: 0.6 }),
		),
		t(
			"last-space-rubber",
			"swipe past the last space",
			{ zone: "space", steps: 0, edge: "last" },
			sp({ travel: -140, ms: 200, momentum: 0.6 }),
		),
		t(
			"swipe-back-during-settle",
			"commit next, then 90 ms after the lift swipe back while the settle runs: ends where it began",
			{ zone: "space", steps: 0, gestures: 2 },
			chain([
				{ events: sp({ travel: -230, ms: 120, peakAt: 0.35, momentum: 0, seed: 11 }) },
				{ events: sp({ travel: 230, ms: 120, peakAt: 0.35, momentum: 0, seed: 12 }), gap: 90 },
			]),
		),
		t(
			"over-native-view",
			"swipe with the pointer over a terminal/browser native view in the content",
			{ zone: "history", steps: 0 },
			synth({ x: native.x, y: native.y, seed: 13, travel: -130, ms: 200, momentum: 0.5 }),
		),
		t(
			"tab-flick-next",
			"fast flick over the tab strip",
			{ zone: "tab", steps: 1 },
			synth({ x: strip.x, y: strip.y, seed: 14, travel: -90, ms: 90, peakAt: 0.35, momentum: 1.2 }),
		),
		t(
			"tab-drag-past-half",
			"slow drag over the strip content",
			{ zone: "tab", steps: 1 },
			synth({ x: content.x, y: content.y, seed: 15, travel: -260, ms: 800, momentum: 0 }),
		),
		t(
			"tab-many-tabs",
			"swipe in a group with 22 tabs (strip overflows horizontally): still switches by one",
			{ zone: "tab", steps: 1 },
			synth({ x: strip.x, y: strip.y, seed: 16, travel: -230, ms: 240, momentum: 0.6 }),
		),
		t(
			"no-zone-outside",
			"swipe over a titlebar / non-gesture area",
			{ zone: "none", steps: 0, untouched: true },
			synth({ x: 700, y: 5, seed: 17, travel: -130, ms: 200, momentum: 0.5 }),
		),
		t(
			"wheel-mouse",
			"a notched wheel mouse (not precise): never a swipe",
			{ zone: "space", steps: 0, untouched: true },
			synth({ x: sidebar.x, y: sidebar.y, seed: 18, travel: -120, ms: 200, momentum: 0, precise: false }),
		),
		// #498: the tab zone is the whole group, editor included. What the editor under the pointer can still do
		// decides, at the landing, whether a sideways gesture is its scroll or a tab swipe.
		t(
			"editor-scroll-owned",
			"fingers left over an editor with more line to the right: the editor's scroll, never a tab swipe",
			{ zone: "tab", steps: 0, untouched: true, scroller: { left: true, right: true } },
			synth({ x: strip.x, y: strip.y, seed: 19, travel: -230, ms: 300, momentum: 0.6 }),
		),
		t(
			"editor-scroll-back-owned",
			"fingers right over an editor scrolled to its right end: it scrolls back, a tab swipe does not start",
			{ zone: "tab", steps: 0, untouched: true, scroller: { left: true, right: false } },
			synth({ x: strip.x, y: strip.y, seed: 20, travel: 230, ms: 300, momentum: 0.6 }),
		),
		t(
			"editor-at-edge-hands-over",
			"fingers left over an editor already at its right end: nothing left to scroll, the tab swipe takes it",
			{ zone: "tab", steps: 1, scroller: { left: true, right: false } },
			synth({ x: strip.x, y: strip.y, seed: 21, travel: -230, ms: 300, momentum: 0.6 }),
		),
	];
}
