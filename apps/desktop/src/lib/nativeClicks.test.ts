// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { deliverNativeClick } from "./nativeClicks.ts";

// Incident (2026-10-26): the new-tab menu stayed open after a click on the terminal below it. A click on a native view
// never reaches the DOM, so the kit's outside-press handler never ran. The page re-delivers it as a pointerdown.
test("a click on a native view reaches the page as pointerdown + mousedown at that point, outside every overlay", () => {
	const slot = document.createElement("div");
	document.body.append(slot);
	const seen: string[] = [];
	const at: [number, number][] = [];
	for (const t of ["pointerdown", "mousedown"])
		document.addEventListener(t, (e) => {
			seen.push(`${t}:${e.target === slot ? "slot" : "other"}`);
			at.push([e.clientX, e.clientY]);
		});
	document.elementFromPoint = () => slot;
	deliverNativeClick({ x: 40, y: 50 }, (r) => r);
	expect(seen).toEqual(["pointerdown:slot", "mousedown:slot"]);
	expect(at[0]).toEqual([40, 50]);
	slot.remove();
});

test("window points are converted to CSS px (layout zoom) before hit-testing", () => {
	let asked: [number, number] = [0, 0];
	document.elementFromPoint = (x, y) => ((asked = [x, y]), document.body);
	deliverNativeClick({ x: 100, y: 60 }, (r) => ({ ...r, x: r.x / 2, y: r.y / 2 }));
	expect(asked).toEqual([50, 30]);
});
