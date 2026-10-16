/**
 * Where the webview keeps its own context menu on top of the kit's text fields and `.ca-selectable`: a CodeMirror editor
 * (gutters, panels), and any element in a debug build, whose Rust bridge sets `__ATLAS_DEBUG__` so Inspect Element works.
 */
export function allowContextMenu(e: MouseEvent): boolean {
	if ("__ATLAS_DEBUG__" in window) return true;
	return e.target instanceof Element && !!e.target.closest(".cm-editor");
}
