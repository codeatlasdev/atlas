// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

// Incident (codeatlasdev/atlas#504): in the editor status bar "UTF-8" was drawn a few pixels higher than "Spaces: 2",
// "LF" and "JSON". UTF-8 is a plain text item (a <span>), the others are <button>s. A button centres its text in its box
// by itself; a span puts the text line at the top of it. The two agree only when the line box is as tall as the box
// (`line-height == height`), and the item rule broke that: `line-height: 18px` came BEFORE `font: inherit`, and the `font`
// shorthand resets `line-height`, so every item had the bar's `line-height: 1` (11.5px) in an 18px box: 3px higher for the
// span, centred for the buttons.
//
// happy-dom has no layout, so this recomputes the one thing the baseline depends on from the REAL compiled CSS of the
// bar and of its item primitive, with the real rendered DOM: which rules reach each item (by `matches`, in source
// order, specificity ignored: the rules here are single-class), the `font` shorthand reset, and `inherit`. The numbers
// were also read off real layout in WebKit (Playwright): before, the span's text top sat 3px above the buttons' (3.25 on
// the 11.5px font); after, 0.
const { flushSync, mount, unmount } = await import("svelte");
const { compile } = await import("svelte/compiler");
const { default: Harness } = await import("../../../test/StatusBarHarness.svelte");

const dir = new URL(".", import.meta.url);
const sources = ["StatusBar.svelte", "StatusItem.svelte", "../lsp/LspStatus.svelte"]
	.map((f) => new URL(f, dir))
	.filter((u) => existsSync(u));
const css = sources
	.map((u) => compile(readFileSync(u, "utf8"), { filename: u.pathname, generate: "client" }).css?.code ?? "")
	.join("\n")
	.replace(/\/\*[\s\S]*?\*\//g, "")
	// Container queries only hide items; they never change a visible item's box.
	.replace(/@container[^{]*\{(?:[^{}]*\{[^}]*\})*[^{}]*\}/g, "");

const rules = [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(([, sel, body]) => ({
	selectors: sel
		.split(",")
		.map((s) => s.trim())
		.filter((s) => s && !s.includes(":")),
	decls: body
		.split(";")
		.map((d) => d.split(/:(.+)/).map((x) => x?.trim()))
		.filter(([p, v]) => p && v),
}));

/** The declared value of `prop` on `el`, from every matching rule in source order; `font` resets the font longhands. */
function declared(el, prop) {
	let value;
	for (const r of rules) {
		if (!r.selectors.some((s) => el.matches(s))) continue;
		for (const [p, v] of r.decls) {
			if (p === prop) value = v;
			else if (p === "font" && ["line-height", "font-size"].includes(prop))
				value = v === "inherit" ? "inherit" : undefined;
		}
	}
	return value;
}
/** `line-height` / `font-size` inherit; everything else does not. */
function computed(el, prop) {
	const v = declared(el, prop);
	if (v === undefined || v === "inherit")
		return el.parentElement ? computed(el.parentElement, prop) : undefined;
	return v;
}
const px = (v) => parseFloat(v);
function lineBox(el) {
	const lh = computed(el, "line-height");
	const fs = px(computed(el, "font-size"));
	return /px$/.test(lh) ? px(lh) : px(lh) * fs;
}

const input = (over = {}) => ({
	line: 12,
	col: 5,
	selections: { count: 1, chars: 0, lines: 1 },
	indent: { style: "space", size: 2, detected: true },
	eol: "lf",
	encoding: "utf-8",
	language: { id: "json", label: "JSON" },
	problems: { errors: 1, warnings: 0 },
	tier: "normal",
	wrap: true,
	...over,
});

function items(over) {
	const target = document.createElement("div");
	document.body.append(target);
	// A language-server document the way `LspDoc` exposes it (a file outside every worktree: "Outside a project").
	const doc = { path: "/x/a.json", languageId: "json", replied: true, status: "ready", supported: {} };
	const app = flushSync(() => mount(Harness, { target, props: { input: input(over), doc } }));
	const bar = target.querySelector("[role=status]");
	// Every item, whatever its element: a button, or a span that is an item (`item` / `status-item`), never a wrapper.
	const found = [...bar.querySelectorAll("button, span")].filter(
		(el) =>
			el.textContent.trim() && (el.matches("button") || /(^|\s)(status-)?item(\s|$)/.test(el.className)),
	);
	return { found, done: () => (unmount(app), target.remove()) };
}

test("every status item, text or button, has a line box as tall as its box so the text sits on one baseline", () => {
	const { found, done } = items();
	const texts = found.map((el) => el.textContent.trim());
	// The bar is VS Code's: a text item (encoding, wrap), buttons (position, indent, EOL, language), a problem count.
	for (const want of ["UTF-8", "LF", "JSON", "Spaces: 2", "Wrap"]) expect(texts).toContain(want);
	expect(found.some((el) => el.tagName === "SPAN")).toBe(true);
	expect(found.some((el) => el.tagName === "BUTTON")).toBe(true);
	// The language-server item (a kit Popover trigger spread onto the primitive) is one of them, whatever its label says.
	expect(found.some((el) => el.getAttribute("aria-label")?.startsWith("Language server"))).toBe(true);
	for (const el of found) {
		const height = px(declared(el, "height"));
		expect({ item: el.textContent.trim(), tag: el.tagName, lineBox: lineBox(el) }).toEqual({
			item: el.textContent.trim(),
			tag: el.tagName,
			lineBox: height,
		});
	}
	done();
});

test("every status item shares one box: same height, display, vertical padding and font size", () => {
	const { found, done } = items();
	const box = (el) => [
		declared(el, "height"),
		declared(el, "display"),
		(declared(el, "padding") ?? "").split(/\s+/)[0],
		computed(el, "font-size"),
	];
	const first = box(found[0]);
	expect(first.every((v) => v !== undefined)).toBe(true);
	for (const el of found)
		expect({ item: el.textContent.trim(), box: box(el) }).toEqual({
			item: el.textContent.trim(),
			box: first,
		});
	done();
});
