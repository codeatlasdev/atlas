/**
 * The built-in kinds, as the reconciler sees them (`reconcileNatives`, `surface.ts`). Imported once by the shell
 * (`shell/session.svelte.ts`), so the reconciler knows them even before any terminal or browser component has been
 * loaded: a view that outlived a webview reload has to be found by a page that has not mounted its panes yet.
 *
 * A new kind adds its own `registerNativeKind` call here (or next to its adapter): the `list` of its live views on the
 * Rust side, and what to do with one nobody owns.
 */
import { invoke } from "@tauri-apps/api/core";
import { registerNativeKind } from "./surface.ts";

// A terminal view nobody owns is closed: its pty lives on in hostd, and a remount reopens the view over it.
registerNativeKind({
	kind: "terminal",
	list: () => invoke<string[]>("term_list"),
	orphan: (tab) => invoke("term_close", { tab }),
});

// A browser view nobody owns is parked, never closed: its page survives (the tab may only be re-parented).
registerNativeKind({
	kind: "browser",
	list: () => invoke<string[]>("browser_list"),
	orphan: (tab) => invoke("browser_park", { tab }),
});
