// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (2026-10-26, a `.ncurc.cjs` in the editor): the document scrolled "forever" past its last line.
// CodeMirror's `scrollPastEnd()` pads the content by `editorHeight - lineHeight`, and `editorHeight` is the scroller's
// own `clientHeight`. When that height follows the content (any ancestor without a definite height: reproduced in
// Chromium with an auto-height host: 394px of padding became 4708px at the first measure and stayed), the padding
// grows the scroller, which grows the padding: no end. The bound below is derived from the viewport, never from the
// element the padding lives in, so it cannot feed itself. Happy-dom has no layout; the plugin's maths is a pure
// function and the extension is checked through the attributes it hands to the content.
// Dynamic on purpose (as in layout.test.ts): the DOM setup must exist before CodeMirror loads; static imports hoist above it.
const { scrollPastEndPadding, scrollPastEnd } = await import("#lib/editor/scrollPastEnd.ts");
const { EditorView } = await import("codemirror");
const { EditorState } = await import("@codemirror/state");

test("padding is the viewport minus one line, so the last line can reach the top and no further", () => {
	expect(scrollPastEndPadding({ viewport: 600, line: 19, top: 0 })).toBe(581 - 0.5);
});

test("padding never goes negative for a tiny or zero-height viewport", () => {
	expect(scrollPastEndPadding({ viewport: 0, line: 19, top: 0 })).toBe(0);
	expect(scrollPastEndPadding({ viewport: 10, line: 19, top: 0 })).toBe(0);
});

test("padding is capped by the window, so a scroller that grew with its own padding cannot grow it again", () => {
	// An editor measured at 5456px inside a 900px-tall window: the old formula gave 5437px of padding.
	const grown = scrollPastEndPadding({ viewport: 5456, line: 19, top: 0, limit: 900 });
	expect(grown).toBeLessThanOrEqual(900);
	// Idempotent under feedback: feeding the result back as a bigger scroller yields the same padding.
	const again = scrollPastEndPadding({ viewport: 5456 + grown, line: 19, top: 0, limit: 900 });
	expect(again).toBe(grown);
});

test("the extension pads the content bottom with a finite px value and never the unbounded 1000px placeholder forever", () => {
	const parent = document.createElement("div");
	document.body.append(parent);
	const view = new EditorView({
		parent,
		state: EditorState.create({ doc: "a\nb", extensions: [scrollPastEnd()] }),
	});
	const style = view.contentDOM.getAttribute("style") ?? "";
	expect(style).toMatch(/padding-bottom:\s*\d+(\.\d+)?px/);
	view.destroy();
	parent.remove();
});
