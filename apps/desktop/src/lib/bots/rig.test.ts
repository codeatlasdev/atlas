// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { fromChannels, pose, spring, STIFFNESS, toChannels } from "./rig.ts";

describe("rig", () => {
	it("every channel has a spring and survives the round trip", () => {
		const p = pose("working", "wrench", 1.3);
		expect(toChannels(p).length).toBe(STIFFNESS.length);
		expect(fromChannels(toChannels(p))).toEqual(p);
	});

	it("needs_you lifts off the floor; offline sinks", () => {
		expect(Math.min(...[0.1, 0.2, 0.3].map((t) => pose("needs_you", "none", t).y))).toBeLessThan(-30);
		expect(pose("offline", "none", 0).y).toBeGreaterThan(0);
	});

	it("spring settles on the target and survives a huge frame gap", () => {
		let [x, v] = [0, 0];
		for (let i = 0; i < 240; i++) [x, v] = spring(x, v, 10, 1 / 60);
		expect(x).toBeCloseTo(10, 2);
		const [y] = spring(0, 0, 10, 5);
		expect(Number.isFinite(y) && Math.abs(y) < 20).toBe(true);
	});
});
