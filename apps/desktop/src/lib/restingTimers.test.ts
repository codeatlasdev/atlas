// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Glob } from "bun";
import { readFileSync } from "node:fs";

// #289 (docs/perf/linux-webkit-study.md): WebKitGTK on Wayland never reports a blurred window, or one on another workspace,
// as hidden, so a raw `setInterval` keeps waking the page for as long as the app is open (the crew board, the automations
// list, the GitHub App card and the pairing QR each ran a 1 s timer of their own). Periodic work goes through
// `lib/clock.svelte.ts` (`clock.every`, `usePoll`), which sleeps once the window has been blurred past its grace.
// A raw interval needs a reason here: it is NOT a loop of the main window, or it must go on while the window rests.
const ALLOWED: Record<string, string> = {
	"lib/clock.svelte.ts": "the clock itself: the one timer everything else shares",
	"lib/capture/cursor.ts":
		"scripted recordings (timer-driven because WebKit throttles rAF while a capture runs)",
	"lib/capture/dom.ts": "scripted recordings",
	"lib/components/terminal/pointer.ts":
		"autoscroll while the pointer is held at an edge: user input drives it",
	"lib/panel/metrics.svelte.ts":
		"samples the history the charts draw: a sleeping window must not leave gaps in it",
	"lib/stores/autoCleanup.svelte.ts": "a 15 min maintenance sweep",
	"lib/stores/preview.svelte.ts": "heartbeat that keeps a live preview server's lease",
	"lib/stores/updates.svelte.ts": "the hourly update check",
	"routes/dock/+page.svelte": "another webview (the dock window) with its own page and no clock",
	"routes/shots/quick/+page.svelte": "another webview (the quick-shot overlay)",
};

test("every raw setInterval in the app is on the allow-list, and the list names no stale file", () => {
	const root = new URL("../", import.meta.url).pathname;
	const found: string[] = [];
	for (const f of new Glob("**/*.{ts,svelte}").scanSync(root)) {
		if (/\.test\.ts$/.test(f)) continue;
		if (/\bsetInterval\(/.test(readFileSync(root + f, "utf8"))) found.push(f);
	}
	expect(found.filter((f) => !(f in ALLOWED))).toEqual([]);
	expect(Object.keys(ALLOWED).filter((f) => !found.includes(f))).toEqual([]);
});
