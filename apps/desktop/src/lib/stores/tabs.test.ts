// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import {
	CLOSED_MAX,
	closable,
	firstMovable,
	mruNext,
	pinned,
	pinnedFirst,
	pushClosed,
	snapshot,
	tabAt,
} from "./tabs.ts";

const tabs = {
	p: { id: "p", kind: "services", worktreeId: "w" },
	a: { id: "a", kind: "terminal", worktreeId: "w" },
	b: { id: "b", kind: "editor", worktreeId: "w" },
	c: { id: "c", kind: "browser", worktreeId: "w" },
};
const of = (id) => tabs[id];

test("only the Panel is pinned", () => {
	expect(pinned(tabs.p)).toBe(true);
	expect(["a", "b", "c"].some((id) => pinned(tabs[id]))).toBe(false);
	expect(pinned(undefined)).toBe(false);
});

test("pinned tabs sort first and movable tabs start after them", () => {
	expect(pinnedFirst(["a", "p", "b"], of)).toEqual(["p", "a", "b"]);
	expect(firstMovable(["p", "a", "b"], of)).toBe(1);
	expect(firstMovable(["a", "b"], of)).toBe(0);
});

test("close-others and close-all never include the Panel", () => {
	expect(closable(["p", "a", "b", "c"], of)).toEqual(["a", "b", "c"]);
	expect(closable(["p", "a", "b", "c"], of, "b")).toEqual(["a", "c"]);
	expect(closable(["p"], of)).toEqual([]);
});

test("⌘1…8 pick by position, ⌘9 the last tab", () => {
	const order = ["p", "a", "b", "c"];
	expect(tabAt(order, 1)).toBe("p");
	expect(tabAt(order, 3)).toBe("b");
	expect(tabAt(order, 8)).toBeUndefined();
	expect(tabAt(order, 9)).toBe("c");
	expect(tabAt([], 9)).toBeUndefined();
	expect(tabAt(order, 0)).toBeUndefined();
	expect(tabAt(order, 10)).toBeUndefined();
});

test("closed tabs are remembered newest last, capped, singletons skipped", () => {
	expect(snapshot({ kind: "services", label: "Panel" }, 0)).toBeNull();
	const t = snapshot({ kind: "editor", label: "a.ts", path: "src/a.ts" }, 2);
	expect(t).toMatchObject({ kind: "editor", path: "src/a.ts", index: 2 });
	let stack = [];
	for (let i = 0; i < CLOSED_MAX + 5; i++)
		stack = pushClosed(stack, { kind: "editor", label: `${i}`, index: i });
	expect(stack.length).toBe(CLOSED_MAX);
	expect(stack[stack.length - 1].label).toBe(`${CLOSED_MAX + 4}`);
	const term = snapshot({ kind: "terminal", label: "Terminal", argv: ["zsh"], pty: "x" }, 1);
	expect(term.argv).toEqual(["zsh"]);
	expect("pty" in term).toBe(false);
});

test("MRU stepping wraps and needs two entries", () => {
	expect(mruNext(["a"], "a", 1)).toBeUndefined();
	expect(mruNext(["a", "b", "c"], "a", 1)).toBe("b");
	expect(mruNext(["a", "b", "c"], "a", -1)).toBe("c");
	expect(mruNext(["a", "b", "c"], "zzz", 1)).toBe("b");
});
