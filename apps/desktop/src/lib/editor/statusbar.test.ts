// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Dynamic on purpose: the Svelte loader registered by test/setup.ts must exist before the component loads.
const { flushSync, mount, unmount } = await import("svelte");
const { default: StatusBar } = await import("./StatusBar.svelte");

const input = (over = {}) => ({
	line: 12,
	col: 5,
	selections: { count: 1, chars: 0, lines: 1 },
	indent: { style: "space", size: 2, detected: true },
	eol: "lf",
	encoding: "utf-8",
	language: { id: "typescript", label: "TypeScript" },
	problems: { errors: 0, warnings: 0 },
	tier: "normal",
	wrap: false,
	...over,
});

function render(props) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(StatusBar, { target, props }));
	return { target, done: () => unmount(app) };
}

test("renders the compact items in VS Code wording", () => {
	const { target, done } = render({ input: input(), onaction: () => {} });
	const text = target.textContent.replace(/\s+/g, " ");
	for (const want of ["Ln 12, Col 5", "Spaces: 2", "UTF-8", "LF", "TypeScript"]) expect(text).toContain(want);
	done();
});

test("clicking an action item reports its id; plain items are not buttons", () => {
	const seen = [];
	const { target, done } = render({ input: input(), onaction: (id) => seen.push(id) });
	const buttons = [...target.querySelectorAll("button")];
	buttons.find((b) => b.textContent.includes("Spaces")).click();
	buttons.find((b) => b.textContent.includes("Ln 12")).click();
	buttons.find((b) => b.textContent === "LF").click();
	expect(seen).toEqual(["indent", "position", "eol"]);
	expect(buttons.some((b) => b.textContent === "UTF-8")).toBe(false);
	done();
});

test("problems and a large-file tier show as warnings; mixed endings are flagged", () => {
	const { target, done } = render({
		input: input({ problems: { errors: 2, warnings: 1 }, tier: "large", eol: "mixed" }),
		onaction: () => {},
	});
	const text = target.textContent;
	expect(text).toContain("2 errors, 1 warning");
	expect(text).toContain("Large file");
	expect(text).toContain("Mixed");
	expect(target.querySelector(".error")?.textContent).toContain("2 errors");
	done();
});

test("a selection count and size show next to the position", () => {
	const { target, done } = render({
		input: input({ selections: { count: 3, chars: 24, lines: 2 } }),
		onaction: () => {},
	});
	expect(target.textContent).toContain("Ln 12, Col 5 (3 selections) (24 selected)");
	done();
});

// Regression: 1.5.0-alpha.26, in a ~450px pane "2 errors, 0 warnings" printed over "Spaces: 4". The bar hides items
// by rank as it narrows (CSS container queries, which the real WebKit proof covers); the contract tested here is the
// order: what the person acts on (caret, problems) never ranks as droppable, encoding and line endings go first.
test("the bar ranks what to drop first: encoding and EOL, then indent, then language; caret and problems never", () => {
	const { statusRank } = require("./statusbar.ts");
	const rank = Object.fromEntries(
		["position", "problems", "indent", "encoding", "eol", "language", "lsp", "wrap"].map((id) => [
			id,
			statusRank(id),
		]),
	);
	expect(rank.position).toBe(0);
	expect(rank.problems).toBe(0);
	expect(rank.encoding).toBeGreaterThan(rank.indent);
	expect(rank.eol).toBe(rank.encoding);
	expect(rank.indent).toBeGreaterThan(rank.language);
	expect(rank.language).toBeGreaterThan(0);
	expect(rank.lsp).toBe(rank.language);
});

test("every rendered cell carries its rank so the narrow rules can reach it", () => {
	const { target, done } = render({
		input: input({ problems: { errors: 2, warnings: 0 } }),
		onaction: () => {},
	});
	const ranks = Object.fromEntries(
		[...target.querySelectorAll("[data-rank]")].map((e) => [
			e.textContent.trim(),
			e.getAttribute("data-rank"),
		]),
	);
	expect(ranks["2 errors, 0 warnings"]).toBe("0");
	expect(ranks["Ln 12, Col 5"]).toBe("0");
	expect(ranks["UTF-8"]).toBe("3");
	expect(ranks["Spaces: 2"]).toBe("2");
	expect(ranks["TypeScript"]).toBe("1");
	done();
});
