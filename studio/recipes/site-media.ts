// The site's six app loops and five screenshots, recorded from the real app, no agent needed:
//
//   bun studio run recipes/site-media.ts            (add --force to re-record)
//   bun studio loops                                 cut + encode loop-*.mp4/.webm/-poster.jpg and shot-*.jpg into site/public/media
//
// Each capture starts a fresh app (Atlas Dark, the app's default theme: the seeded settings set no theme) from a fixture (capture/fixtures.ts) and a
// script that drives it through its own stores: no OS input, no window on the user's desktop, no recording indicator
// (the recorder is ScreenCaptureKit inside the app, with the window buttons hidden). What each file is made of is in
// src/site-media.ts. Scripts port the 1.0 scenes of src/capture/scenes/*.ts, same pacing.
import { RECIPE_SCHEMA, type RecipeInput } from "../src/run/recipe.ts";

/** 16:10, the largest window the agent display admits: it is 1280x820 pt, the window keeps 4 pt clear of every edge and starts below the 30 pt menu-bar band (`agent_mode::target_frame`; 1272x784 is already hidden by the watchdog). */
const size = { width: 1248, height: 780 } as const;
const base = { version: 1, size, fps: 60, cursor: false } as const;
/** The Logs tab's search box and its regex toggle (aria-labels of Logs.svelte). */
const search = 'input[aria-label="Search logs"]';
const regex = 'button[aria-label="Regular expression"]';

const recipe: RecipeInput = {
	schema: RECIPE_SCHEMA,
	id: "site-media",
	title: "Site loops and screenshots",
	stages: ["capture"],
	captures: [
		{
			// switch spaces (the strip at the bottom of the sidebar, then the shortcuts), hide the sidebar and peek it back
			id: "workspace",
			fixture: "orbit-spaces",
			shots: ["workspace"],
			script: {
				...base,
				steps: [
					{ do: "pause", ms: 1200 },
					{ do: "shot", name: "workspace" },
					{ do: "click", selector: '.spaces .space[aria-label="nova-docs"]' },
					{ do: "pause", ms: 1400 },
					{ do: "click", selector: '.spaces .space[aria-label="pulse-api"]' },
					{ do: "pause", ms: 1400 },
					{ do: "key", combo: "alt-cmd-left" },
					{ do: "pause", ms: 1300 },
					{ do: "key", combo: "alt-cmd-left" },
					{ do: "pause", ms: 1300 },
					{ do: "click", selector: '.spaces .space[aria-label="orbit-shop"]' },
					{ do: "pause", ms: 1500 },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "pause", ms: 1800 },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "pause", ms: 1500 },
					{ do: "key", combo: "alt-cmd-right" },
					{ do: "pause", ms: 1300 },
				],
			},
		},
		{
			// the live project panel: health pills, per-service CPU/memory sparklines, resource charts
			id: "panel",
			fixture: "orbit-stack",
			shots: ["panel"],
			script: {
				...base,
				steps: [
					{ do: "worktree", project: "orbit-shop" },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "tab", kind: "services" },
					{ do: "pause", ms: 2500 },
					{ do: "shot", name: "panel" },
					{ do: "scroll", selector: ".scroll", to: 380, ms: 2200 },
					{ do: "pause", ms: 2500 },
					{ do: "scroll", selector: ".scroll", to: 0, ms: 1800 },
					{ do: "pause", ms: 1500 },
				],
			},
		},
		{
			// the Logs tab: find with highlights, regex, level filter, time range
			id: "logs",
			fixture: "orbit-stack",
			shots: ["logs"],
			script: {
				...base,
				steps: [
					{ do: "worktree", project: "orbit-shop" },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "tab", kind: "logs" },
					{ do: "pause", ms: 1300 },
					{ do: "type", selector: search, text: "stripe", delayMs: 110 },
					{ do: "pause", ms: 1800 },
					{ do: "shot", name: "logs" },
					{ do: "key", combo: "enter" },
					{ do: "pause", ms: 900 },
					{ do: "click", selector: regex },
					{ do: "type", selector: search, text: "|timeout", delayMs: 80 },
					{ do: "pause", ms: 1700 },
					{ do: "click", selector: regex },
					{ do: "click", text: "warn+" },
					{ do: "pause", ms: 2000 },
					{ do: "click", text: "error+" },
					{ do: "pause", ms: 1800 },
					{ do: "click", text: "all levels" },
					{ do: "pause", ms: 900 },
					{ do: "click", selector: 'button[aria-label^="Time range"]' },
					{ do: "pause", ms: 1000 },
					{ do: "click", text: "Last 1 min" },
					{ do: "pause", ms: 1500 },
				],
			},
		},
		{
			// the GPU terminal demo: ligatures, emoji, truecolor, text styles, a kitty-protocol image
			id: "terminal",
			fixture: "orbit-stack",
			shots: ["terminal"],
			script: {
				...base,
				steps: [
					{ do: "worktree", project: "orbit-shop" },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "terminal", argv: ["bash", "-c", "bash ~/.atlas-studio/term-demo"], label: "Terminal" },
					{ do: "wait", terminal: { contains: "ligatures", timeoutMs: 15000 } },
					{ do: "pause", ms: 6500 },
					{ do: "shot", name: "terminal" },
					{ do: "pause", ms: 6000 },
				],
			},
		},
		{
			// three scripted agents in worktrees, working / blocked / done, in the sidebar and the Agents view
			id: "agents",
			fixture: "orbit-agents",
			shots: ["agents"],
			script: {
				...base,
				steps: [
					{ do: "pause", ms: 3500 },
					{ do: "view", view: "agents" },
					{ do: "pause", ms: 4500 },
					{ do: "shot", name: "agents" },
					{ do: "worktree", project: "orbit-shop", name: "fix-stripe-timeout" },
					{ do: "pause", ms: 4500 },
					{ do: "view", view: "agents" },
					{ do: "pause", ms: 3500 },
					{ do: "worktree", project: "orbit-shop", name: "cart-empty-state" },
					{ do: "pause", ms: 3000 },
				],
			},
		},
		{
			// a terminal running mcp-demo.py: the real `atlas mcp` over stdio, the agent side scripted
			id: "mcp",
			fixture: "orbit-stack",
			script: {
				...base,
				steps: [
					{ do: "worktree", project: "orbit-shop" },
					{ do: "key", combo: "ctrl-cmd-s" },
					{ do: "terminal", argv: ["bash", "-c", "bash ~/.atlas-studio/mcp-demo"], label: "omp" },
					{ do: "wait", terminal: { contains: "atlas_status", timeoutMs: 20000 } },
					{ do: "pause", ms: 14000 },
				],
			},
		},
	],
};
export default recipe;
