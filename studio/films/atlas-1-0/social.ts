// The social renditions of the 1.0 film: ONE source cut (`social`, 30 s, six beats: scenes + what the viewer must READ in
// each) and a derived cut per aspect (9:16, 1:1, 4:5). Nothing here places a pixel: `reframeCut` (src/camera/reframe.ts)
// lays out each aspect, plans the virtual camera from the focuses against the profile's legibility floor, and
// `studio fix` tunes zooms through camera.json. The footage and the voice are the film's own (same recordings, the
// cached narration of the 1.0 vertical cut), so a render needs no capture and no ElevenLabs call.

import type { AppClipScene, Cut, NarrationLine, Scene, TerminalScene, TitleScene, VoiceSpec } from "../../src/lib/types.ts";

const APP_CROP = { x: 0, y: 4, w: 2496, h: 1556 };
/** `box` is ignored by a camera scene (the viewport replaces it); it is the source cut's own 16:9 placement. */
const BOX = { x: "center", y: "center", w: 1500, h: 936 } as const;

const beat = (id: string, clip: string, from: number, focus: NonNullable<AppClipScene["focus"]>, headline: { title: string; subtitle: string }): AppClipScene => ({
	id,
	type: "app",
	clip: `rec:site-media/${clip}`,
	from,
	duration: 5,
	bg: "v",
	crop: APP_CROP,
	box: BOX,
	focus,
	overlay: { ...headline, y: 0, titleSize: 100, subtitleSize: 64 },
});

// What each beat's narration says, and where the viewer must look (text on screen, found by OCR on the recording:
// these clips predate the recipe `focus` step; recipes that add it use `selector:` instead). `textPx` is the CSS px of
// the smallest text in the thing to read: the app's labels are 11-14 px.
const scenes: Scene[] = [
	beat("v1", "workspace", 2.5, [{ id: "space", at: 0.9, hold: 1.4, text: "Other projects", textPx: 13, label: "every project is a space" }, { id: "up", at: 3.1, hold: 1.4, text: "Dashboard", textPx: 13, label: "one dashboard per project" }], { title: "Atlas", subtitle: "One file. Whole stack." }),
	{
		id: "v2",
		type: "terminal",
		take: "take:tui",
		segments: [[19.0, 24.0]],
		width: 972,
		duration: 5,
		bg: "v",
		overlay: { title: "atlas dev", subtitle: "Your stack, in a TUI", y: 0, titleSize: 100, subtitleSize: 64 },
	} satisfies TerminalScene,
	beat("v3", "terminal", 7.5, [{ id: "gpu", at: 0.9, hold: 1.2, text: "truecolor", textPx: 13, label: "truecolor, ligatures" }, { id: "img", at: 2.9, hold: 1.2, text: "(kitty graphics)", textPx: 13, label: "inline images" }], { title: "A GPU terminal", subtitle: "emoji and images" }),
	beat("v4", "panel", 2.5, [{ id: "kpi", at: 0.7, hold: 1.2, text: "Restarts", textPx: 13, label: "restarts, last hour" }, { id: "failed", at: 2.8, hold: 1.2, text: "gave up after", textPx: 13, label: "payments failed" }], { title: "Live panel", subtitle: "health, CPU, memory" }),
	beat("v5", "agents", 4, [{ id: "agent", at: 0.9, hold: 1.4, text: "fix-stripe-timeou", textPx: 13, label: "agents in worktrees" }, { id: "count", at: 3.1, hold: 1.4, text: "cart-empty-stat", textPx: 13, label: "each in its own worktree" }], { title: "Agents", subtitle: "in their own worktrees" }),
	{
		id: "v6",
		type: "title",
		duration: 5,
		bg: "v",
		transitionIn: { type: "dip", duration: 0.3 },
		fadeOut: 0.5,
		mark: { size: 300, y: 420, delay: 0, fade: 0.01 },
		text: { title: "Open source. Free.", subtitle: "atlas.codeatlas.com.br", y: 720, titleSize: 120 },
	} satisfies TitleScene,
];

// The cached takes of the published vertical cut (line id + text key the voice cache): 0.55 s into each 5 s beat.
const lines: NarrationLine[] = [
	{ id: "v1", at: 0.55, text: "Atlas. Your whole dev stack, in one file." },
	{ id: "v2", at: 5.55, text: "A fast terminal dashboard with live search." },
	{ id: "v3", at: 10.55, text: "A GPU terminal, with emoji and inline images." },
	{ id: "v4", at: 15.55, text: "A live panel for health, CPU and memory." },
	{ id: "v5", at: 20.55, text: "Agents, each in its own worktree." },
	{ id: "v6", at: 25.55, text: "Atlas is open source. Install it today." },
];

const voice: VoiceSpec = {
	id: "cjVigY5qzO86Huf0OWal",
	tag: "[confident, warm, modern product launch narrator]",
	stability: 0.5,
	similarity: 0.7,
	model: "eleven_v4",
	lufs: -14,
	lines,
};

/** The source: scenes and focuses only. Never rendered itself (`Film.sources`). */
export const socialSource: Cut = { aspect: "16:9", out: "social", scenes, voice, captions: { vtt: false, burned: false } };

const derived = (aspect: Cut["aspect"], out: string, profile: string, style: NonNullable<Cut["captions"]["style"]>): Cut => ({
	aspect,
	out,
	scenes: [],
	voice,
	captions: { vtt: false, burned: true, style },
	reframe: { from: "social", profile },
	poster: 6,
});

export const socialRenditions: Cut[] = [
	derived("9:16", "film-social-vertical", "social-vertical", "reels"),
	derived("1:1", "film-social-square", "social-square", "feed"),
	derived("4:5", "film-social-portrait", "social-portrait", "feed"),
];
