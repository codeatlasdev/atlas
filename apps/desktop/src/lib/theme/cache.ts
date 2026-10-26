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
}

let last = "";

/** Persist the applied theme for the next launch. Identical payloads are skipped. */
export function writeThemeCache(cache: ThemeCache) {
	const key = JSON.stringify(cache);
	if (key === last) return;
	last = key;
	void invoke("theme_cache_write", { cache }).catch(() => {});
}
