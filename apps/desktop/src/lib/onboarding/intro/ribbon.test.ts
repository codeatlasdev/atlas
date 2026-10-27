// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { camera } from "./ribbon.ts";

describe("camera: the punch on the completion hit", () => {
	test("rests at 1 before the hit", () => {
		expect(camera(0).zoom).toBe(1);
		expect(camera(0).shakeX).toBe(0);
	});
	test("the hit punches the camera in (zoom > 1) and shakes, then both decay to rest", () => {
		const hit = camera(1.7);
		expect(hit.zoom).toBeGreaterThan(1.02);
		const mag = (c) => Math.hypot(c.shakeX, c.shakeY);
		expect(Math.max(...[1.69, 1.71, 1.73, 1.76, 1.8].map((t) => mag(camera(t))))).toBeGreaterThan(0.5);
		expect(mag(camera(2.4))).toBeLessThan(0.01);
		expect(camera(2.4).zoom).toBeLessThan(hit.zoom);
	});
	test("shake is deterministic (screenshots and the live view agree)", () => {
		expect(camera(1.74)).toEqual(camera(1.74));
	});
	test("reduced shake scales it down without changing the zoom punch", () => {
		const full = camera(1.72, 1);
		const calm = camera(1.72, 0);
		expect(Math.hypot(calm.shakeX, calm.shakeY)).toBe(0);
		expect(calm.zoom).toBeCloseTo(full.zoom, 5);
	});
});
