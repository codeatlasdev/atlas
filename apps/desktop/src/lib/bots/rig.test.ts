// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, it } from "bun:test";
import { equipEnter, equipImpulse, fromChannels, pose, spring, STIFFNESS, toChannels } from "./rig.ts";

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

	it("equip impulse has one kick per channel and hops the body up", () => {
		for (const part of ["hat", "outfit", "tool", "face", "kit", "color", "eyes"] as const) {
			const k = equipImpulse(part);
			expect(k.length).toBe(STIFFNESS.length);
			expect(k[0]).toBeLessThan(0);
		}
	});

	it("equip impulse really hops the sprung body, then it lands back", () => {
		let ch = toChannels(pose("idle", "none", 0));
		const vel = equipImpulse("hat");
		let minY = 0;
		for (let i = 0; i < 180; i++) {
			ch = ch.map((x, j) => {
				const [nx, nv] = spring(x, vel[j], toChannels(pose("idle", "none", 0))[j], 1 / 60, ...STIFFNESS[j]);
				vel[j] = nv;
				return nx;
			});
			minY = Math.min(minY, ch[0]);
		}
		expect(minY).toBeLessThan(-30);
		expect(ch[0]).toBeCloseTo(0, 0);
	});

	it("equipEnter starts displaced and settles to identity", () => {
		expect(equipEnter("hat", 0).dy).toBeLessThan(-100);
		expect(equipEnter("outfit", 0).reveal).toBe(0);
		expect(equipEnter("tool", 0).scale).toBeLessThan(1);
		expect(equipEnter("face", 0).scale).toBeGreaterThan(1);
		for (const part of ["hat", "outfit", "tool", "face"] as const)
			expect(equipEnter(part, 2)).toEqual({ dy: 0, dx: 0, reveal: 1, scale: 1, alpha: 1 });
	});
});
