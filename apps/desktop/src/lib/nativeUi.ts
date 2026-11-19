/**
 * The page's own routes to native UI, so that they go through the host's gate (`crates/atlas-desktop/src/native_ui.rs`):
 * an agent instance must never raise a window, panel or other app outside its own window.
 *
 * - An `<a target="_blank">` opens the browser through `open_url`. (Tauri's shell plugin used to do this behind the
 *   page's back, with no gate.)
 * - An agent instance cannot open the OS file panel an `<input type="file">` raises: it would appear on the user's
 *   display, not the virtual one.
 */
import { api } from "#lib/api.ts";

/** What the app hands to the browser: web pages and mail links (the same set `open_url` accepts). */
const OPENED = ["http://", "https://", "mailto:"];

/** The URL a click on `<a target="_blank">` stands for; `null` for any other click, or one already handled. */
export function externalHref(e: Pick<MouseEvent, "defaultPrevented" | "target">): string | null {
	if (e.defaultPrevented) return null;
	const a = e.target instanceof Element ? e.target.closest("a") : null;
	if (!a || a.target !== "_blank" || !OPENED.some((p) => a.href.startsWith(p))) return null;
	return a.href;
}

/** Open `target=_blank` links in the browser, once, after the component's own handlers have had their say. */
export function openExternalLinks(
	doc: Document = document,
	open: (url: string) => Promise<void> = api.openUrl,
) {
	doc.addEventListener("click", (e) => {
		const href = externalHref(e);
		if (!href) return;
		e.preventDefault();
		open(href).catch((err) => console.error(`could not open ${href}:`, err));
	});
}

/** Agent instance: a click that would open the OS file panel does nothing but say so (the debug log shows it). */
export function blockFilePickers(doc: Document, report: (message: string) => void) {
	doc.addEventListener(
		"click",
		(e) => {
			const t = e.target;
			if (!(t instanceof HTMLInputElement) || t.type !== "file") return;
			e.preventDefault();
			e.stopImmediatePropagation();
			report(
				"native file picker suppressed in an agent instance (it would have appeared on the user's screen)",
			);
		},
		true,
	);
}
