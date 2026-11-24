// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { fromCapability, refusal, withSetting, type Capability } from "./gpuCapability.ts";

/** What Rust sends (`serde(rename_all = "camelCase")` of `Capability` in term/linux/capability.rs). */
const reply = (over: Partial<Capability> = {}): Capability => ({
	available: true,
	reason: null,
	adapter: "AMD Radeon 680M (radv, Vulkan)",
	setting: false,
	active: false,
	...over,
});

describe("fromCapability", () => {
	test("a capable machine with the setting off offers the toggle and renders with xterm.js", () => {
		const s = fromCapability(reply());
		expect(s).toEqual({
			supported: false,
			offered: true,
			terminal: false,
			restartNeeded: false,
			reason: null,
		});
	});

	test("the native terminal renders only when it was on at launch", () => {
		expect(fromCapability(reply({ setting: true, active: true })).supported).toBe(true);
		// Turned on this run: the window was not made see-through, so xterm.js until a restart.
		const s = fromCapability(reply({ setting: true, active: false }));
		expect(s.supported).toBe(false);
		expect(s.terminal).toBe(true);
		expect(s.restartNeeded).toBe(true);
	});

	test("turning it off while it is active asks for a restart and stops nothing already drawn", () => {
		const s = fromCapability(withSetting(reply({ setting: true, active: true }), false));
		expect(s.restartNeeded).toBe(true);
		expect(s.terminal).toBe(false);
	});

	test("an unavailable machine (X11, XWayland, no Vulkan, NVIDIA proprietary) offers nothing, even with the setting on", () => {
		const s = fromCapability(
			reply({
				available: false,
				reason: "not a Wayland session",
				adapter: null,
				setting: true,
				active: false,
			}),
		);
		expect(s).toEqual({
			supported: false,
			offered: false,
			terminal: false,
			restartNeeded: false,
			reason: "not a Wayland session",
		});
	});

	test("the reply Rust serializes is what this reads", () => {
		// The producer's field names, not a hand-written mock of them: a rename on either side fails here.
		const rust = JSON.parse(
			'{"available":true,"reason":null,"adapter":"gpu","setting":true,"active":true}',
		) as Capability;
		expect(fromCapability(rust).supported).toBe(true);
	});
});

// Phase 3 fallback proof (no-Vulkan session): the user turned the fast terminal on, the machine cannot run it, and every
// terminal quietly stayed xterm.js. One toast must say why; a machine that was never asked says nothing.
describe("refusal", () => {
	test("asked for and refused: the reason, so the page can say why xterm.js is in use", () => {
		expect(refusal(reply({ available: false, reason: "no usable GPU: llvmpipe", setting: true }))).toBe(
			"no usable GPU: llvmpipe",
		);
	});

	test("not asked, or capable: nothing to say", () => {
		expect(refusal(reply({ available: false, reason: "not a Wayland session", setting: false }))).toBeNull();
		expect(refusal(reply({ setting: true, active: true }))).toBeNull();
		expect(refusal(reply({ setting: true, active: false }))).toBeNull();
	});
});
