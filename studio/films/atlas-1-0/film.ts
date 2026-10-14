// The Atlas 1.0 launch film: 16:9 (77.93 s) and the 9:16 social cut (30 s). Scene order, durations, crops, the TUI cut
// and the narration beats are the ones of the film published with 1.0 (site/public/media/film.mp4), ported from the
// ad-hoc ffmpeg scripts. Provenance of the media is in site/public/media/README.md.

import type { AppClipScene, Chapter, Cut, Film, NarrationLine, Scene, TerminalScene, TitleScene, VoiceSpec } from "../../src/lib/types.ts";
import { socialRenditions, socialSource } from "./social.ts";

// ---- 16:9 -------------------------------------------------------------------------------------------------------

/**
 * App footage is the six captures of `recipes/site-media.ts` (`bun studio run`): the real app in agent mode, Atlas Dark (the default theme),
 * with the window buttons hidden by the recorder (no OS "being shared" pill). Each frame is 2496x1560 (the 1248x780 window the agent display admits, 2x): the window
 * (2496x1556) starts at row 4, the 4 rows above are the wallpaper of the private virtual display. Cropping the
 * window EXACTLY keeps its bezel, corners and titlebar whole and leaves out the sliver, which sat above the dark
 * bezel and read as structure crossing the window's top edge (`frame/window-cropped` at 38.97 s and 58.45 s).
 * `film.test.ts` measures that row on the footage each scene plays. The box gives >= 3% headroom (the lint's rule).
 */
const APP_CROP = { x: 0, y: 4, w: 2496, h: 1556 };
/** 1500 wide at the window's aspect is 936 high: 72 px of headroom above and below in a 1080 frame (6.7%). */
const APP_BOX = { x: "center", y: "center", w: 1500, h: 936 } as const;
/** The window's own corner radius is ~28 px at 2x; at the 1500/2496 scale that is ~17 px. */
const APP_RADIUS = 17;

/** Section card: headline and subtitle centered over a still of the field, 1.7 s. */
const card = (id: string, bg: string, title: string, subtitle: string, titleSize = 104, y = 440): TitleScene => ({
	id,
	type: "title",
	duration: 1.7,
	bg,
	zoom: 0.0005,
	text: { title, subtitle, y, titleSize, delay: 0.15, fade: 0.4 },
});

const app = (id: string, clip: string, from: number, duration: number): AppClipScene => ({
	id,
	type: "app",
	clip: `rec:site-media/${clip}`,
	from,
	duration,
	bg: "a",
	crop: APP_CROP,
	box: APP_BOX,
	radius: APP_RADIUS,
});

const tui: TerminalScene = {
	id: "tui",
	type: "terminal",
	take: "take:tui",
	// the dense cut of the vhs take (demo/demo.tape): boot, error jumps, links, find, level + time filters, action
	segments: [[10.6, 13.8], [19.0, 21.6], [24.2, 26.6], [29.0, 31.0], [33.2, 36.2]],
	speed: 1.5,
	width: 1500,
	duration: 8,
	bg: "c",
};

const yaml = `name: orbit-shop
services:
  api:
    command: bun run dev
    port: 4100
    ready_log: listening
    depends_on:
      migrate: completed_successfully
  web:
    command: bun run dev
    port: 4101
    depends_on: { api: log_ready }
  worker:
    command: bun run worker
    restart: on_failure
actions:
  - key: x
    label: Seed demo data
    confirm: "Insert fake orders?"`;

const scenes169: Scene[] = [
	{
		id: "intro",
		type: "title",
		duration: 4.5,
		bg: "a",
		zoom: 0.0005,
		transitionIn: { type: "cut", duration: 0 },
		fadeOut: 0.4,
		mark: { size: 300, y: 250, delay: 0.3, fade: 1 },
		text: { title: "Atlas", subtitle: "Your whole dev stack. One file.", y: 650, titleSize: 150, delay: 1.4, fade: 0.8 },
	},
	{
		id: "yaml",
		type: "code",
		duration: 4.5,
		bg: "b",
		zoom: 0.0004,
		transitionIn: { type: "dip", duration: 0.3 },
		fadeOut: 0.4,
		heading: { text: "One atlas.yaml", y: 56, size: 68 },
		file: "atlas.yaml",
		lang: "yaml",
		code: yaml,
		y: 170,
		width: 980,
		fontSize: 28,
		delay: 0.2,
		fade: 0.5,
	},
	card("c02", "c", "atlas dev", "A fast terminal dashboard for the whole stack"),
	tui,
	card("c03", "a", "Atlas Desktop", "Every project is a space"),
	app("spaces", "workspace", 2.5, 6),
	card("c04", "b", "A GPU terminal", "Ligatures, emoji, inline images"),
	app("term", "terminal", 1.5, 7),
	card("c05", "c", "Live project panel", "Health, CPU and memory, as it happens"),
	app("panel", "panel", 2.2, 6.0333),
	card("c06", "a", "Logs you can search", "Regex, levels, time ranges"),
	app("logs", "logs", 2.5, 8),
	card("c07", "b", "Agents in worktrees", "Run many, see who needs you"),
	app("agents", "agents", 3.5, 8),
	card("c08", "c", "MCP built in", "Your agent sees the same stack you do"),
	app("mcp", "mcp", 1.5, 9),
	{
		id: "outro",
		type: "title",
		duration: 5,
		bg: "a",
		zoom: 0.0004,
		transitionIn: { type: "dip", duration: 0.5 },
		fadeOut: 0.6,
		mark: { size: 220, y: 200, delay: 0, fade: 0.01 },
		text: { title: "Open source. Free.", subtitle: "Atlas", url: "atlas.codeatlas.com.br", y: 520, titleSize: 130, delay: 0.3, fade: 0.7 },
	},
];

// Voice: Eleven v4, "Eric - Smooth, Trustworthy". `at` is when each line starts in the film.
const narration169: NarrationLine[] = [
	{ id: "n01", at: 0.45, text: "Atlas. Your whole dev stack, in one file." },
	{ id: "n02", at: 4.75, text: "Services, ports, health and actions. One atlas.yaml." },
	{ id: "n03", at: 9.7, text: "Run atlas dev for a fast terminal dashboard." },
	{ id: "n04", at: 13.11, text: "Search every service with a live match counter. Jump between errors, follow links, filter by level or time." },
	{ id: "n05", at: 20.0, text: "Atlas Desktop. Every project is a space." },
	{ id: "n06", at: 23.95, text: "Switch spaces in one move. Hide the sidebar, and peek it back." },
	{ id: "n07", at: 28.75, text: "A GPU terminal with ligatures, color emoji and inline images." },
	{ id: "n08", at: 35.85, text: "A live project panel. Health, CPU and memory, as it happens." },
	{ id: "n09", at: 42.95, text: "Logs you can search. Regex, levels, time ranges. Find the error in seconds." },
	{ id: "n10", at: 52.95, text: "Agents in their own worktrees. See who's working, blocked, or done." },
	{ id: "n11", at: 62.55, text: "MCP is built in. Your agent sees the same stack you do." },
	{ id: "n12", at: 73.35, text: "Atlas is open source. Install it today." },
];

const voiceBase: Omit<VoiceSpec, "lines"> = {
	id: "cjVigY5qzO86Huf0OWal",
	tag: "[confident, warm, modern product launch narrator]",
	stability: 0.5,
	similarity: 0.7,
	model: "eleven_v4",
	lufs: -14,
};

/** Chapters follow the beats of the film; the site player reads them from site/public/media/film-chapters.json. */
const chapters169: Chapter[] = [
	{ t: 0, title: "One file" },
	{ t: 9.7, title: "atlas dev in the terminal" },
	{ t: 20, title: "Spaces and the sidebar" },
	{ t: 28.7, title: "GPU terminal" },
	{ t: 35.85, title: "Project panel" },
	{ t: 42.95, title: "Logs" },
	{ t: 52.95, title: "Agents in worktrees" },
	{ t: 62.55, title: "MCP" },
	{ t: 73.2, title: "Get Atlas" },
];

// ---- 9:16 -------------------------------------------------------------------------------------------------------

/**
 * 972 of 1080 px: 54 px per side. The lint wants >= 3% of the short side (33 px), and it measures the window together
 * with its soft shadow (~20 px); the 1000 px box (40 px by arithmetic) measured 20 and failed (film.test.ts).
 */
const V_BOX = { x: "center", y: 700, w: 972 } as const;

/** Clip over the vertical field with the headline over it: six 5 s beats. */
const vApp = (id: string, clip: string, from: number, title: string, subtitle: string, titleSize: number, y: number, subSize = 80): AppClipScene => ({
	id,
	type: "app",
	clip: `rec:site-media/${clip}`,
	from,
	duration: 5,
	bg: "v",
	crop: APP_CROP,
	box: V_BOX,
	radius: 12,
	overlay: { title, subtitle, y, titleSize, subtitleSize: subSize },
});

const scenes916: Scene[] = [
	vApp("v1", "workspace", 2.5, "Atlas", "One file. Whole stack.", 150, 240),
	{
		id: "v2",
		type: "terminal",
		take: "take:tui",
		segments: [[19.0, 24.0]],
		width: V_BOX.w,
		y: 700,
		duration: 5,
		bg: "v",
		overlay: { title: "atlas dev", subtitle: "Your stack, in a TUI", y: 260, titleSize: 100, subtitleSize: 80 },
	},
	vApp("v3", "terminal", 7.5, "A GPU terminal", "emoji and images", 100, 260),
	vApp("v4", "panel", 2.5, "Live panel", "health, CPU, memory", 100, 260),
	vApp("v5", "agents", 4, "Agents", "in their own worktrees", 100, 260),
	{
		id: "v6",
		type: "title",
		duration: 5,
		bg: "v",
		transitionIn: { type: "dip", duration: 0.3 },
		fadeOut: 0.5,
		mark: { size: 300, y: 420, delay: 0, fade: 0.01 },
		text: { title: "Open source. Free.", subtitle: "atlas.codeatlas.com.br", y: 720, titleSize: 120 },
	},
];

// Start of each line within its 5 s beat, recovered by correlating the voice lines against the published vertical film's audio (0.55 s).
const narration916: NarrationLine[] = [
	{ id: "v1", at: 0.55, text: "Atlas. Your whole dev stack, in one file." },
	{ id: "v2", at: 5.55, text: "A fast terminal dashboard with live search." },
	{ id: "v3", at: 10.55, text: "A GPU terminal, with emoji and inline images." },
	{ id: "v4", at: 15.55, text: "A live panel for health, CPU and memory." },
	{ id: "v5", at: 20.55, text: "Agents, each in its own worktree." },
	{ id: "v6", at: 25.55, text: "Atlas is open source. Install it today." },
];

const horizontal: Cut = {
	aspect: "16:9",
	out: "film",
	scenes: scenes169,
	voice: { ...voiceBase, lines: narration169 },
	chapters: chapters169,
	captions: { vtt: true, burned: false },
	poster: 41,
};

const vertical: Cut = {
	aspect: "9:16",
	out: "film-vertical",
	scenes: scenes916,
	voice: { ...voiceBase, lines: narration916 },
	captions: { vtt: false, burned: false },
	poster: 7,
};

const film: Film = {
	id: "atlas-1-0",
	title: "Atlas 1.0",
	fps: 30,
	crf: 26,
	loudness: { target: -14, tolerance: 0.7 },
	music: {
		bed: "music:bed",
		bedLufs: -20,
		fadeIn: 2,
		fadeOut: 4,
		duck: { threshold: 0.03, ratio: 2.5, attack: 80, release: 600 },
		source: "ElevenLabs Music (music_v1), instrumental, generated for this film",
		license: "Generated music, used under the ElevenLabs plan terms of the account that generated it; verify those terms cover your distribution before using the film outside this site.",
	},
	cuts: [horizontal, vertical],
	sources: [socialSource],
	renditions: socialRenditions,
};

export default film;
