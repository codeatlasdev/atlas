/**
 * Make the webview behave like a macOS app, not a web page:
 * - focus rings only while navigating with the keyboard (WebKit treats programmatic
 *   focus, e.g. a dialog focusing its close button, as `:focus-visible`);
 * - no browser context menu (Reload / Inspect Element) outside text fields;
 * - no browser page zoom: pinch is blocked, and ⌘+ / ⌘- / ⌘0 are swallowed here so WebKit never zooms by
 *   itself (the app's own keyboard zoom is `zoom.ts`, stepped and persisted), plus no dragging of links or images.
 * Returns a cleanup function.
 */
export function nativeFeel(): () => void {
	const root = document.documentElement;
	const keyboard = (e: KeyboardEvent) => {
		if (e.key === "Tab" || e.key.startsWith("Arrow")) root.dataset.kbd = "";
	};
	const pointer = () => delete root.dataset.kbd;
	const menu = (e: MouseEvent) => {
		// Debug builds (the Rust bridge sets this flag) keep WebKit's menu for Inspect Element.
		if ("__ATLAS_DEBUG__" in window) return;
		const t = e.target as HTMLElement | null;
		const editable = t?.closest("input, textarea, [contenteditable], .cm-editor, .selectable");
		if (!editable) e.preventDefault();
	};
	const zoom = (e: KeyboardEvent) => {
		if (e.metaKey && ["=", "+", "-", "0"].includes(e.key)) e.preventDefault();
	};
	const pinch = (e: WheelEvent) => {
		if (e.ctrlKey) e.preventDefault();
	};
	const drag = (e: DragEvent) => {
		if ((e.target as HTMLElement | null)?.closest?.("a, img")) e.preventDefault();
	};

	window.addEventListener("keydown", keyboard, true);
	window.addEventListener("keydown", zoom, true);
	window.addEventListener("pointerdown", pointer, true);
	window.addEventListener("contextmenu", menu);
	window.addEventListener("wheel", pinch, { passive: false });
	window.addEventListener("dragstart", drag);
	return () => {
		window.removeEventListener("keydown", keyboard, true);
		window.removeEventListener("keydown", zoom, true);
		window.removeEventListener("pointerdown", pointer, true);
		window.removeEventListener("contextmenu", menu);
		window.removeEventListener("wheel", pinch);
		window.removeEventListener("dragstart", drag);
	};
}
