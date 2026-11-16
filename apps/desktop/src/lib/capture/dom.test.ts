// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, describe, expect, test } from "bun:test";

// The DOM setup above must exist before dom.ts runs `instanceof HTMLInputElement`; static imports are hoisted.
const dom = await import("./dom.ts");
const { clickBySelector, clickByText, elementRect, findByText, pressKey, typeInto } = dom;

function html(s: string) {
	document.body.innerHTML = s;
}
/** happy-dom does no layout; give elements a rect and the visibility WebKit would report (`checkVisibility`). */
function laidOut(el: Element, r = { x: 10, y: 20, width: 100, height: 30 }) {
	el.getBoundingClientRect = () => ({
		...r,
		left: r.x,
		top: r.y,
		right: r.x + r.width,
		bottom: r.y + r.height,
		toJSON() {},
	});
	el.checkVisibility = () => true;
}
/** What WebKit says of `display: none` (itself or an ancestor): no box, not rendered. */
function hidden(el: Element) {
	el.checkVisibility = () => false;
}

beforeEach(() => html(""));

describe("typeInto", () => {
	test("the text ends up in the field, one input event per character, the way a keyboard produces it", async () => {
		html('<input class="q">');
		const el = document.querySelector(".q");
		const seen: string[] = [];
		el.addEventListener("input", (e) => seen.push(`${e.data}:${el.value}`));
		const clock: number[] = [];
		await typeInto(".q", "orb", 0, (ms) => (clock.push(ms), Promise.resolve()));
		expect(el.value).toBe("orb");
		expect(seen).toEqual(["o:o", "r:or", "b:orb"]);
		expect(clock).toEqual([0, 0, 0]);
	});

	test("appends to what is there and focuses the field", async () => {
		html('<input class="q" value="x">');
		await typeInto(".q", "y", 0, () => Promise.resolve());
		expect(document.querySelector(".q").value).toBe("xy");
		expect(document.activeElement).toBe(document.querySelector(".q"));
	});

	test("works on a textarea and refuses a non-field with the selector in the message", async () => {
		html('<textarea class="t"></textarea><div class="d"></div>');
		await typeInto(".t", "hi", 0, () => Promise.resolve());
		expect(document.querySelector(".t").value).toBe("hi");
		await expect(typeInto(".d", "x", 0, () => Promise.resolve())).rejects.toThrow(/\.d/);
		await expect(typeInto(".missing", "x", 0, () => Promise.resolve())).rejects.toThrow(/\.missing/);
	});

	test("characters outside the BMP (emoji) are typed whole, not split into surrogates", async () => {
		html('<input class="q">');
		await typeInto(".q", "a😀b", 0, () => Promise.resolve());
		expect(document.querySelector(".q").value).toBe("a😀b");
	});
});

describe("clicks", () => {
	test("clickBySelector clicks the first visible match and reports a miss", () => {
		html('<button class="a">A</button>');
		const b = document.querySelector(".a");
		laidOut(b);
		let n = 0;
		b.addEventListener("click", () => n++);
		expect(clickBySelector(".a")).toBe(true);
		expect(n).toBe(1);
		expect(clickBySelector(".nope")).toBe(false);
	});

	test("a hidden element (display: none) is not clicked: a script must not press what the viewer cannot see", () => {
		html('<button class="a">A</button>');
		const b = document.querySelector(".a");
		hidden(b);
		let n = 0;
		b.addEventListener("click", () => n++);
		expect(clickBySelector(".a")).toBe(false);
		expect(n).toBe(0);
	});

	// Incident (macOS release check, 2026-11-15): visibility was `offsetParent !== null`, which WebKit answers null
	// for every `position: fixed` element and for <body>, so a script could never press a button in a fixed
	// dialog or sheet. Rendered means "has a box", whatever its positioning.
	test("a button in a fixed overlay (offsetParent null in WebKit) is clicked", () => {
		html('<div class="sheet"><button class="ok">OK</button></div>');
		const b = document.querySelector(".ok");
		laidOut(b);
		Object.defineProperty(b, "offsetParent", { get: () => null, configurable: true });
		let n = 0;
		b.addEventListener("click", () => n++);
		expect(clickBySelector(".ok")).toBe(true);
		expect(n).toBe(1);
	});

	test("clickByText matches the whole trimmed text, not a substring", () => {
		html("<button>Start all</button><button> Start </button>");
		for (const b of document.querySelectorAll("button")) laidOut(b);
		const hits: string[] = [];
		for (const b of document.querySelectorAll("button"))
			b.addEventListener("click", () => hits.push(b.textContent.trim()));
		expect(clickByText("Start")).toBe(true);
		expect(hits).toEqual(["Start"]);
		expect(clickByText("tart")).toBe(false);
	});

	test("findByText only considers visible elements", () => {
		html("<span>Hi</span>");
		hidden(document.querySelector("span"));
		expect(findByText("Hi")).toBeUndefined();
		laidOut(document.querySelector("span"));
		expect(findByText("Hi")?.tagName).toBe("SPAN");
	});
});

describe("pressKey", () => {
	test("dispatches a keydown with the parsed modifiers to the focused element and reports preventDefault", () => {
		html('<input class="q">');
		const q = document.querySelector(".q");
		q.focus();
		let ev;
		q.addEventListener("keydown", (e) => {
			ev = e;
			e.preventDefault();
		});
		expect(pressKey("cmd-shift-p")).toBe(true);
		expect(ev.key).toBe("p");
		expect(ev.metaKey && ev.shiftKey).toBe(true);
		expect(ev.ctrlKey || ev.altKey).toBe(false);
	});

	test("falls back to the body when nothing is focused", () => {
		let hit = false;
		document.body.addEventListener("keydown", () => (hit = true));
		pressKey("escape");
		expect(hit).toBe(true);
	});
});

describe("elementRect", () => {
	test("returns the box in CSS px, or undefined when the element is missing", () => {
		html('<div class="x"></div>');
		laidOut(document.querySelector(".x"), { x: 5, y: 6, width: 70, height: 80 });
		expect(elementRect(".x")).toEqual({ x: 5, y: 6, width: 70, height: 80 });
		expect(elementRect(".y")).toBeUndefined();
	});
});

describe("waitForQuiet (a shell is ready when its output stops changing, not after a fixed sleep)", () => {
	const { waitForQuiet } = dom;

	test("resolves once the text has not changed for quietMs, and reports how long it took", async () => {
		let t = 0;
		const reads = ["", "warn", "warn\nprompt", "warn\nprompt"];
		let i = 0;
		const took = await waitForQuiet(
			async () => reads[Math.min(i++, reads.length - 1)],
			{ quietMs: 300, timeoutMs: 10_000, pollMs: 100 },
			{ now: () => t, sleep: async (ms) => void (t += ms) },
		);
		expect(took).toBeGreaterThanOrEqual(300);
		expect(took).toBeLessThan(10_000);
	});

	test("a terminal that never prints anything is 'quiet' only after output appeared: empty output keeps waiting", async () => {
		let t = 0;
		await expect(
			waitForQuiet(
				async () => "",
				{ quietMs: 200, timeoutMs: 1000, pollMs: 100 },
				{ now: () => t, sleep: async (ms) => void (t += ms) },
			),
		).rejects.toThrow(/no output/);
	});

	test("output that keeps changing past the timeout fails instead of typing into a shell that is still starting", async () => {
		let t = 0;
		let n = 0;
		await expect(
			waitForQuiet(
				async () => String(n++),
				{ quietMs: 300, timeoutMs: 1000, pollMs: 100 },
				{ now: () => t, sleep: async (ms) => void (t += ms) },
			),
		).rejects.toThrow(/still changing/);
	});
});
