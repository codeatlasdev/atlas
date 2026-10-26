/**
 * Scroll past the last line, bounded. Replaces CodeMirror's `scrollPastEnd()`, whose padding is
 * `editorHeight - lineHeight` with `editorHeight` read back from the scroller the padding lives in: if that
 * height ever follows its content (an ancestor without a definite height) each measure adds the padding to the
 * height and the height to the padding, and the document scrolls without end. Here the padding is also capped by
 * the window's height, a number the padding cannot change, so the loop has a fixed point: at most about one
 * viewport past the end, like VS Code's `scrollBeyondLastLine`.
 */
import { type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";

/** Bottom padding that lets the last line reach the top of a `viewport`-tall editor, never above `limit`. */
export function scrollPastEndPadding({
	viewport,
	line,
	top,
	limit = Number.POSITIVE_INFINITY,
}: {
	viewport: number;
	line: number;
	top: number;
	limit?: number;
}): number {
	return Math.max(0, Math.min(viewport - line - top - 0.5, limit));
}

const plugin = ViewPlugin.fromClass(
	class {
		padding = 0;
		attrs = { style: "padding-bottom: 0px" };
		update({ view }: ViewUpdate) {
			const padding = scrollPastEndPadding({
				viewport: view.scrollDOM.clientHeight,
				line: view.defaultLineHeight,
				top: view.documentPadding.top,
				limit: (view.dom.ownerDocument.defaultView ?? window).innerHeight,
			});
			if (padding !== this.padding) {
				this.padding = padding;
				this.attrs = { style: `padding-bottom: ${padding}px` };
			}
		}
	},
);

export function scrollPastEnd(): Extension {
	return [plugin, EditorView.contentAttributes.of((view) => view.plugin(plugin)?.attrs ?? null)];
}
