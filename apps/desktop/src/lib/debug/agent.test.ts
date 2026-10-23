// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { beforeEach, describe, expect, test } from "bun:test";

const agent = await import("./agent.ts");
const {
	a11yTree,
	clickAt,
	clickTarget,
	pressKey,
	elementShotRect,
	fullShotSize,
	stateSummary,
	typeTarget,
	waitFor,
} = agent;

const html = (s: string) => (document.body.innerHTML = s);
/** happy-dom does no layout: give elements a rect and visibility the way WebKit would. */
function laidOut(el: Element, r = { x: 10, y: 20, width: 100, height: 30 }) {
	el.getBoundingClientRect = () => ({
		...r,
		left: r.x,
		top: r.y,
		right: r.x + r.width,
		bottom: r.y + r.height,
		toJSON() {},
	});
	Object.defineProperty(el, "offsetParent", { get: () => document.body, configurable: true });
}
beforeEach(() => html(""));

describe("waitFor", () => {
	const clock = () => {
		let t = 0;
		return { now: () => t, sleep: async (ms: number) => void (t += ms) };
	};

	test("resolves with the time it took as soon as the selector matches", async () => {
		html("");
		const c = clock();
		let polls = 0;
		const r = await waitFor(
			{ selector: ".ready" },
			{ timeoutMs: 5000, pollMs: 100 },
			{
				...c,
				sleep: async (ms) => {
					await c.sleep(ms);
					if (++polls === 3) html('<div class="ready"></div>');
				},
			},
		);
		expect(r).toEqual({ ok: true, ms: 300 });
	});

	test("an expression is awaited too: true when it returns something truthy", async () => {
		const c = clock();
		let n = 0;
		const r = await waitFor({ expr: "window.__n++ >= 2" }, { timeoutMs: 5000, pollMs: 50 }, c, (src) => {
			void src;
			return ++n >= 3;
		});
		expect(r.ok).toBe(true);
	});

	test("a timeout is an error that says what was awaited, never a silent pass", async () => {
		const c = clock();
		await expect(waitFor({ selector: ".never" }, { timeoutMs: 300, pollMs: 100 }, c)).rejects.toThrow(
			/timed out after 300 ms waiting for selector \.never/,
		);
	});

	// Incident: the bridge sends JSON `{"selector":".x","expr":null}`; `null` is not `undefined`, so every `wait`
	// from the CLI died with "wait needs exactly one of selector or expr".
	test("a JSON null for the unused one means 'not given', as the bridge sends it", async () => {
		html('<div class="ready"></div>');
		laidOut(document.querySelector(".ready"));
		const o = { timeoutMs: 50, pollMs: 1 };
		expect((await waitFor({ selector: ".ready", expr: null }, o, clock())).ok).toBe(true);
		expect((await waitFor({ selector: null, expr: "1 + 1" }, o, clock())).ok).toBe(true);
		await expect(waitFor({ selector: null, expr: null }, o, clock())).rejects.toThrow(/exactly one/);
	});

	test("exactly one of selector or expr is required", async () => {
		await expect(waitFor({}, { timeoutMs: 10, pollMs: 5 })).rejects.toThrow(/selector or expr/);
		await expect(waitFor({ selector: "a", expr: "b" }, { timeoutMs: 10, pollMs: 5 })).rejects.toThrow(
			/selector or expr/,
		);
	});
});

describe("clickAt (the mirror's clicks)", () => {
	test("hits the topmost element at the point with the pointer sequence and coordinates", () => {
		html('<div class="card"><button class="go">Go</button></div>');
		const b = document.querySelector(".go");
		document.elementFromPoint = (x, y) => (x === 40 && y === 25 ? b : document.body);
		const seen = [];
		for (const t of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"])
			document.querySelector(".card").addEventListener(t, (e) => seen.push(`${t}@${e.clientX},${e.clientY}`));
		expect(clickAt(40, 25)).toMatchObject({ clicked: true, tag: "button", text: "Go" });
		expect(seen).toEqual([
			"pointerdown@40,25",
			"mousedown@40,25",
			"pointerup@40,25",
			"mouseup@40,25",
			"click@40,25",
		]);
	});

	test("a point outside the page or not a number is reported, not clicked", () => {
		document.elementFromPoint = () => null;
		expect(clickAt(9999, 9999)).toEqual({ clicked: false, reason: "nothing at 9999,9999" });
		expect(clickAt(Number.NaN, 1)).toEqual({ clicked: false, reason: "bad point" });
	});
});

describe("pressKey (the mirror's keyboard)", () => {
	const withTerminal = (writes) => {
		window.__atlas = {
			ws: { selected: { id: "w" }, focusedTab: () => ({ pty: "p1" }) },
			host: { ptyWrite: async (pty, b) => writes.push([pty, b]) },
		};
	};

	test("with no field focused, keys go to the focused terminal as terminal bytes", async () => {
		html("<div></div>");
		const writes = [];
		withTerminal(writes);
		expect(await pressKey({ key: "l" })).toEqual({ to: "terminal", key: "l" });
		await pressKey({ key: "c", ctrl: true });
		await pressKey({ key: "Enter" });
		expect(writes).toEqual([
			["p1", "l"],
			["p1", "\x03"],
			["p1", "\r"],
		]);
	});

	test("a focused text field gets the text, not the terminal behind it", async () => {
		html('<input class="q" value="ab">');
		const q = document.querySelector(".q");
		q.focus();
		q.setSelectionRange(2, 2);
		const writes = [];
		withTerminal(writes);
		const inputs = [];
		q.addEventListener("input", (e) => inputs.push(e.inputType));
		expect((await pressKey({ key: "c" })).to).toBe("field");
		await pressKey({ key: "Backspace" });
		await pressKey({ key: "x" });
		expect(q.value).toBe("abx");
		expect(inputs).toEqual(["insertText", "deleteContentBackward", "insertText"]);
		expect(writes).toEqual([]);
	});

	test("a focused terminal sink gets the keydown to map itself, and printable text through input", async () => {
		html('<textarea aria-label="Terminal input"></textarea>');
		const sink = document.querySelector("textarea");
		sink.focus();
		const writes = [];
		withTerminal(writes);
		const got = [];
		// GpuTerminal: keydown handles control keys (preventDefault), input carries text.
		sink.addEventListener("keydown", (e) => {
			if (e.ctrlKey) {
				got.push(`key ctrl+${e.key}`);
				e.preventDefault();
			}
		});
		sink.addEventListener("input", () => {
			got.push(`text ${sink.value}`);
			sink.value = "";
		});
		expect((await pressKey({ key: "c", ctrl: true })).to).toBe("terminal");
		await pressKey({ key: "x" });
		expect(got).toEqual(["key ctrl+c", "text x"], "Ctrl+C must not also insert a c");
		expect(writes).toEqual([], "the terminal's own path, not a raw pty write");
	});

	test("⌘ shortcuts reach the app's key handlers, never the pty", async () => {
		html("<div></div>");
		const writes = [];
		withTerminal(writes);
		let seen = "";
		document.addEventListener("keydown", (e) => (seen = `${e.metaKey ? "⌘" : ""}${e.key}`), { once: true });
		expect((await pressKey({ key: "k", meta: true })).to).toBe("page");
		expect(seen).toBe("⌘k");
		expect(writes).toEqual([]);
	});
});

describe("clickTarget / typeTarget (DOM events only, never OS input)", () => {
	test("click fires the element's click handler and reports what it hit", () => {
		html('<button class="go">Go</button>');
		const b = document.querySelector(".go");
		laidOut(b);
		let n = 0;
		b.addEventListener("click", () => n++);
		expect(clickTarget(".go")).toMatchObject({ clicked: true, tag: "button", text: "Go" });
		expect(n).toBe(1);
	});

	test("a missing or invisible element is reported, not silently ignored", () => {
		html('<button class="go">Go</button>');
		// display:none in WebKit: no offsetParent
		Object.defineProperty(document.querySelector(".go"), "offsetParent", {
			get: () => null,
			configurable: true,
		});
		expect(clickTarget(".nope")).toEqual({ clicked: false, reason: "no element matches .nope" });
		expect(clickTarget(".go")).toEqual({ clicked: false, reason: ".go is not visible" });
	});

	test("type fills an input through the native setter and fires input events", async () => {
		html('<input class="q">');
		const el = document.querySelector(".q");
		laidOut(el);
		const seen = [];
		el.addEventListener("input", (e) => seen.push(e.data));
		expect(await typeTarget(".q", "ab", 0)).toBe("ab");
		expect(seen).toEqual(["a", "b"]);
	});

	test("type into something that is not a field says so", async () => {
		html('<div class="x"></div>');
		await expect(typeTarget(".x", "a", 0)).rejects.toThrow(/not an input or textarea/);
	});
});

describe("a11yTree", () => {
	test("roles, accessible names and nesting of what a screen reader would see; decoration is dropped", () => {
		html(`<main aria-label="Panel"><h1>Services</h1><button aria-label="Restart api">⟳</button>
			<ul><li>api</li></ul><div aria-hidden="true"><button>hidden</button></div><div class="deco"></div></main>`);
		const tree = a11yTree(document.body);
		const flat = JSON.stringify(tree);
		expect(flat).toContain('"role":"main"');
		expect(flat).toContain('"name":"Panel"');
		expect(flat).toContain('"role":"heading"');
		expect(flat).toContain('"name":"Restart api"');
		expect(flat).toContain('"role":"list"');
		expect(flat).not.toContain("hidden");
	});

	test("a button without any name is flagged: the structural bug a visual check would miss", () => {
		html("<button></button>");
		const [btn] = a11yTree(document.body)
			.flatMap(function walk(n) {
				return [n, ...(n.children ?? []).flatMap(walk)];
			})
			.filter((n) => n.role === "button");
		expect(btn.problem).toBe("button has no accessible name");
	});
});

describe("elementShotRect", () => {
	test("the rect of an element in CSS px, clipped to the viewport, with the device scale", () => {
		html('<div class="card"></div>');
		const el = document.querySelector(".card");
		laidOut(el, { x: 100, y: 50, width: 400, height: 300 });
		expect(elementShotRect(".card", { w: 1280, h: 820, scale: 2 })).toEqual({
			x: 100,
			y: 50,
			width: 400,
			height: 300,
			scale: 2,
		});
	});

	test("an element partly outside the viewport is clipped; fully outside or missing throws a clear error", () => {
		html('<div class="c"></div>');
		laidOut(document.querySelector(".c"), { x: 1200, y: 800, width: 400, height: 300 });
		expect(elementShotRect(".c", { w: 1280, h: 820, scale: 2 })).toMatchObject({
			x: 1200,
			y: 800,
			width: 80,
			height: 20,
		});
		laidOut(document.querySelector(".c"), { x: 2000, y: 0, width: 10, height: 10 });
		expect(() => elementShotRect(".c", { w: 1280, h: 820, scale: 2 })).toThrow(/outside the viewport/);
		// touching the viewport edge exactly is zero area: also outside
		laidOut(document.querySelector(".c"), { x: 1280, y: 0, width: 10, height: 10 });
		expect(() => elementShotRect(".c", { w: 1280, h: 820, scale: 2 })).toThrow(/outside the viewport/);
		expect(() => elementShotRect(".missing", { w: 1, h: 1, scale: 1 })).toThrow(
			/no element matches \.missing/,
		);
	});
});

describe("stateSummary", () => {
	test("a compact, JSON-safe summary of the stores an agent asks 'what is the app doing' with", () => {
		const s = stateSummary({
			ws: { loaded: true, doc: { worktrees: [{ id: "w1", name: "main" }], selected: "w1" } },
			store: {
				loaded: true,
				projects: [{ id: "p", root: "/r", name: "orbit" }],
				selected: "p",
				live: { p: { services: [{ name: "api", state: "running" }] } },
			},
		});
		expect(JSON.parse(JSON.stringify(s))).toEqual({
			loaded: { ws: true, store: true },
			projects: [{ id: "p", name: "orbit", root: "/r", services: { api: "running" } }],
			worktrees: [{ id: "w1", name: "main" }],
			selected: { worktree: "w1", project: "p" },
		});
	});

	test("missing stores degrade to empty parts instead of throwing (the page may still be booting)", () => {
		expect(stateSummary({})).toEqual({
			loaded: { ws: false, store: false },
			projects: [],
			worktrees: [],
			selected: {},
		});
	});
});

describe("perfReport", () => {
	const { perfReport } = agent;

	test("60 fps steady frames: fps, percentiles and no long tasks", () => {
		const r = perfReport(Array(120).fill(16.67), [], "visible");
		expect(r).toMatchObject({
			visibility: "visible",
			frames: 120,
			fps: 60,
			p50_ms: 17,
			max_ms: 17,
			long_tasks: [],
		});
	});

	test("one 200 ms hitch shows in p95/max and lowers fps: the 'feels janky' evidence", () => {
		const gaps = [...Array(94).fill(16.67), ...Array(6).fill(200)];
		const r = perfReport(gaps, [{ start: 5, ms: 200 }], "visible");
		expect(r.max_ms).toBe(200);
		expect(r.p95_ms).toBe(200);
		expect(r.fps).toBeLessThan(40);
		expect(r.long_tasks).toEqual([{ start: 5, ms: 200 }]);
	});

	test("a page that never painted (hidden: rAF throttled) reports 0 fps and says it was hidden, not NaN", () => {
		expect(perfReport([], [], "hidden")).toMatchObject({
			visibility: "hidden",
			frames: 0,
			fps: 0,
			p50_ms: 0,
			max_ms: 0,
		});
	});
});

describe("which element a selector means", () => {
	// Incident: `shot --selector aside.sidebar` failed with "outside the viewport": the first match in the DOM was the
	// off-screen slide-in copy, while click/wait already (rightly) mean "the first VISIBLE match".
	test("shot rect and full size use the first visible match, like click and wait do", () => {
		html('<aside class="s" id="hidden"></aside><aside class="s" id="shown"></aside>');
		const [hidden, shown] = document.querySelectorAll(".s");
		hidden.getBoundingClientRect = () => ({
			x: -300,
			y: 0,
			width: 260,
			height: 700,
			left: -300,
			top: 0,
			right: -40,
			bottom: 700,
			toJSON() {},
		});
		Object.defineProperty(hidden, "offsetParent", { get: () => null, configurable: true });
		laidOut(shown, { x: 0, y: 0, width: 260, height: 700 });
		Object.defineProperty(shown, "scrollHeight", { value: 700, configurable: true });
		Object.defineProperty(shown, "scrollWidth", { value: 260, configurable: true });
		expect(elementShotRect(".s", { w: 1280, h: 820, scale: 2 })).toMatchObject({
			x: 0,
			width: 260,
			height: 700,
		});
		expect(fullShotSize(".s", { w: 1280, h: 820 })).toEqual({ width: 260, height: 700 });
	});

	// Incident (first agent run): a slid-out sidebar has layout (offsetParent set) but sits at x=-260, off the view.
	test("a match that has layout but lies outside the viewport is skipped for one that is on screen", () => {
		html('<aside class="s"></aside><aside class="s"></aside>');
		const [away, here] = document.querySelectorAll(".s");
		laidOut(away, { x: -260, y: 0, width: 260, height: 790 });
		laidOut(here, { x: 8, y: 40, width: 260, height: 742 });
		expect(elementShotRect(".s", { w: 1280, h: 790, scale: 2 })).toMatchObject({
			x: 8,
			y: 40,
			width: 260,
			height: 742,
		});
	});

	test("when every match is off screen the error names where it is", () => {
		html('<aside class="s"></aside>');
		laidOut(document.querySelector(".s"), { x: -260, y: 0, width: 260, height: 790 });
		expect(() => elementShotRect(".s", { w: 1280, h: 790, scale: 2 })).toThrow(
			/outside the viewport.*x=-260/,
		);
	});

	test("when every match is invisible the error says so instead of 'outside the viewport'", () => {
		html('<aside class="s"></aside>');
		const el = document.querySelector(".s");
		Object.defineProperty(el, "offsetParent", { get: () => null, configurable: true });
		expect(() => elementShotRect(".s", { w: 1280, h: 820, scale: 2 })).toThrow(/not visible/);
		expect(() => fullShotSize(".s", { w: 1280, h: 820 })).toThrow(/not visible/);
	});
});

describe("fullShotSize", () => {
	const scrollable = (el: Element, w: number, h: number) => {
		Object.defineProperty(el, "scrollWidth", { value: w, configurable: true });
		Object.defineProperty(el, "scrollHeight", { value: h, configurable: true });
	};

	test("an element's full size is its scroll size, not the clipped box the viewport shows", () => {
		html('<div class="list"></div>');
		const el = document.querySelector(".list");
		laidOut(el, { x: 0, y: 0, width: 400, height: 300 });
		scrollable(el, 400, 2400);
		expect(fullShotSize(".list", { w: 1280, h: 820 })).toEqual({ width: 400, height: 2400 });
	});

	test("no selector means the whole document; a box that is not scrollable keeps its own size", () => {
		html('<div class="box"></div>');
		scrollable(document.documentElement, 1280, 5000);
		expect(fullShotSize(undefined, { w: 1280, h: 820 })).toEqual({ width: 1280, height: 5000 });
		const el = document.querySelector(".box");
		laidOut(el, { x: 0, y: 0, width: 200, height: 100 });
		scrollable(el, 200, 100);
		expect(fullShotSize(".box", { w: 1280, h: 820 })).toEqual({ width: 200, height: 100 });
	});

	test("a runaway page is capped (a snapshot of 200000 px would exhaust memory) and a missing element is an error", () => {
		html('<div class="inf"></div>');
		const el = document.querySelector(".inf");
		laidOut(el, { x: 0, y: 0, width: 400, height: 300 });
		scrollable(el, 400, 200000);
		expect(fullShotSize(".inf", { w: 1280, h: 820 }).height).toBe(16384);
		expect(() => fullShotSize(".missing", { w: 1, h: 1 })).toThrow(/no element matches \.missing/);
	});
});
