// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { Glob } from "bun";
import { readFileSync } from "node:fs";
import { join } from "node:path";

/**
 * The Tauri ACL is checked by the app at run time, so a page that calls a command its window has no permission for
 * fails only in the real app, and Vite/bun never see it. This reads the REAL capability files of the shell and the REAL
 * frontend source, and checks every command the frontend can send against them.
 *
 * Regression: 1.5.0-alpha.27 (#671). `tauri-plugin-dialog` injects a script that replaces `window.alert` and
 * `window.confirm` with `plugin:dialog|message`; the capability grants no dialog (on purpose: native UI goes through
 * `native_ui.rs`), so every `confirm()` of the crew screens was refused with "Command plugin:dialog|message not allowed
 * by ACL". The refusal is a rejected promise, `confirm()` then returned a (truthy) Promise, and "Delete crewmate" ran
 * WITHOUT the question while the user saw a "A background task failed" toast.
 */

const root = join(import.meta.dir, "../../../..");
const granted = new Set<string>();
for (const f of new Glob("*.json").scanSync(join(root, "crates/atlas-desktop/capabilities"))) {
	const cap = JSON.parse(readFileSync(join(root, "crates/atlas-desktop/capabilities", f), "utf8"));
	for (const p of cap.permissions) granted.add(typeof p === "string" ? p : p.identifier);
}

const src = join(root, "apps/desktop/src");

/** Source of the app (tests excluded) with comments blanked, keeping line numbers. */
function sources(): { file: string; text: string }[] {
	const out = [];
	for (const f of new Glob("**/*.{ts,svelte}").scanSync(src)) {
		if (/\.(test|contract\.test|spec)\.ts$/.test(f) || f.includes("paraglide")) continue;
		const text = readFileSync(join(src, f), "utf8")
			.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
			.replace(/<!--[\s\S]*?-->/g, (m) => m.replace(/[^\n]/g, " "))
			.replace(/(^|[^:"'`\\])\/\/.*$/gm, "$1");
		out.push({ file: f, text });
	}
	return out;
}

/** Lines `file:line: text` of every match of `re` in the app's source. */
function find(re: RegExp): string[] {
	const hits = [];
	for (const { file, text } of sources()) {
		text.split("\n").forEach((line, i) => {
			if (re.test(line)) hits.push(`${file}:${i + 1}: ${line.trim()}`);
		});
	}
	return hits;
}

test("the capability files are read and grant the window and notification commands the shell relies on", () => {
	expect(granted.size).toBeGreaterThan(5);
	expect(granted.has("core:default")).toBe(true);
});

test("the page never calls a global the injected dialog or notification plugin turns into a command the capability refuses", () => {
	// What each plugin's injected script maps a global to (tauri-plugin-dialog `init-iife.js`, tauri-plugin-notification).
	const injected: [string, RegExp, string][] = [
		["window.alert", /(?<![\w.$#])(?:window\.|globalThis\.)?alert\s*\(/, "dialog:allow-message"],
		["window.confirm", /(?<![\w.$#])(?:window\.|globalThis\.)?confirm\s*\(/, "dialog:allow-message"],
		["window.Notification", /\bnew\s+(?:window\.)?Notification\s*\(/, "notification:allow-notify"],
	];
	const refused = [];
	for (const [what, re, permission] of injected) {
		if (granted.has(permission)) continue;
		for (const hit of find(re)) refused.push(`${hit}  <- ${what} needs "${permission}"`);
	}
	expect(
		refused,
		"no capability grants these: ask in the app (stores/ask.svelte.ts confirmDialog) or grant the permission",
	).toEqual([]);
});

test("every plugin command the page invokes by name is granted by a capability", () => {
	// Plugins whose commands `core:default` (and the `core:window:allow-*` entries) cover; their commands are API calls
	// of `@tauri-apps/api`, not names the page writes.
	const core: Record<string, true> = {
		event: true,
		window: true,
		webview: true,
		app: true,
		path: true,
		resources: true,
		menu: true,
		tray: true,
		image: true,
	};
	const missing = [];
	for (const { file, text } of sources()) {
		for (const m of text.matchAll(/["'`]plugin:([a-z-]+)\|([a-z_]+)["'`]/g)) {
			const [, plugin, cmd] = m;
			if (core[plugin]) continue;
			const need = `${plugin}:allow-${cmd.replaceAll("_", "-")}`;
			if (!granted.has(need)) missing.push(`${file}: invokes ${m[0]}, needs ${need}`);
		}
	}
	expect(missing).toEqual([]);
});

test("the frontend imports no Tauri plugin package without a capability for it", () => {
	const plugins = new Set<string>();
	for (const { text } of sources())
		for (const m of text.matchAll(/from\s+["']@tauri-apps\/plugin-([a-z-]+)["']/g)) plugins.add(m[1]);
	for (const p of plugins) {
		expect(
			[...granted].some((g) => g.startsWith(`${p}:`)),
			`@tauri-apps/plugin-${p} is imported, no capability grants ${p}:*`,
		).toBe(true);
	}
});
