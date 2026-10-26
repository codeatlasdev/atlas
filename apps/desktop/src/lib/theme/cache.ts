import { invoke } from "@tauri-apps/api/core";

/** What `boot.rs` replays on <html> before the first paint (`theme-cache.json`). */
export interface ThemeCache {
	name: string;
	/** The kit's scheme preference: `dark`, `light` or `system`. */
	scheme: "dark" | "light" | "system";
	/** Whether the applied theme is dark. */
	dark: boolean;
	/** Exactly the custom properties the engine put on <html>. */
	vars: Record<string, string>;
	/** The window ground (`windowGround`, `#rrggbb`): the NSWindow paints it behind the page and the native views. */
	ground: string;
}

let last = "";

/** Persist the applied theme for the next launch. Identical payloads are skipped. */
export function writeThemeCache(cache: ThemeCache) {
	const key = JSON.stringify(cache);
	if (key === last) return;
	last = key;
	void invoke("theme_cache_write", { cache }).catch(() => {});
	// The live window repaints now; the cache only serves the next launch.
	void invoke("window_ground", { hex: cache.ground }).catch(() => {});
}
