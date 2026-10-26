// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (2026-10-26, a `.ncurc.cjs` in the editor): the line was highlighted and the status bar said Ln 7, Col 15,
// but no caret blinked. The window sets `user-select: none` on html/body (app.css) so chrome is not selectable;
// `-webkit-user-select` inherits, and WKWebView will not put a caret or a selection in an editable element that
// inherits `none` (Chromium does, which is why the dev checks in a browser never showed it). Every other text surface
// of the app opts back in (FieldInput, SearchField, Logs…); the editor did not. Layout is not computed in happy-dom,
// so this pins the stylesheet, like layout.test.ts.
// Dynamic on purpose (as in layout.test.ts): the DOM setup must exist before CodeMirror loads; static imports hoist above it.
const { EditorView } = await import("codemirror");
const { EditorState } = await import("@codemirror/state");
const { atlasTheme } = await import("#lib/cm.ts");

function css(): string {
	return [...document.querySelectorAll("style")].map((s) => s.textContent ?? "").join("\n");
}

/** Declarations of every rule whose selector list has an entry ending in `sel` (CM scopes rules under `.ͼN`). */
function rule(sheet: string, sel: string): string {
	const out: string[] = [];
	for (const m of sheet.matchAll(/([^{}]+)\{([^}]*)\}/g)) {
		const selectors = m[1].split(",").map((x) => x.trim());
		if (selectors.some((x) => x.endsWith(sel))) out.push(m[2]);
	}
	return out.join(";");
}

function mount() {
	const parent = document.createElement("div");
	document.body.append(parent);
	const view = new EditorView({ parent, state: EditorState.create({ doc: "x", extensions: [atlasTheme] }) });
	return { view, parent };
}

test("the editable content opts back into text selection, unprefixed and -webkit- (WKWebView needs the prefix)", () => {
	const { view, parent } = mount();
	const content = rule(css(), ".cm-content");
	expect(content).toMatch(/(^|;|\s)user-select:\s*text/);
	expect(content).toMatch(/-webkit-user-select:\s*text/);
	view.destroy();
	parent.remove();
});

test("the caret takes the theme's caret colour on the content and on the drawn cursor, and is wide enough to see", () => {
	const { view, parent } = mount();
	const sheet = css();
	expect(rule(sheet, ".cm-content")).toMatch(/caret-color:\s*var\(--cm-caret/);
	const cursor = rule(sheet, ".cm-cursor");
	expect(cursor).toMatch(/border-left-color:\s*var\(--cm-caret/);
	expect(cursor).toMatch(/border-left-width:\s*2px/);
	view.destroy();
	parent.remove();
});

test("nothing in the theme hides the drawn cursor layer or its blink", () => {
	const { view, parent } = mount();
	const sheet = css();
	for (const sel of [".cm-cursorLayer", ".cm-cursor"]) {
		const r = rule(sheet, sel);
		expect(r).not.toMatch(/visibility:\s*hidden/);
		expect(r).not.toMatch(/opacity:\s*0\b/);
		expect(r).not.toMatch(/animation:\s*none/);
	}
	view.destroy();
	parent.remove();
});
