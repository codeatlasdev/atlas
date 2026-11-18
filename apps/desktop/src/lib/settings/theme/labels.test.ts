// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { realKitModule } from "../../../../test/uiKit.ts";
import { ACCENT_MESSAGES, accentLabel, appearanceLabel } from "./labels.ts";

// The kit's list as data (`dist/tokens/presets.js` is plain JS): the package entry is stubbed in component suites.
const { THEME_PRESETS } = await realKitModule("tokens/presets.js");

describe("accent color names", () => {
	// The kit names its accent colors in English; a color it adds in an update would show untranslated without this.
	test("every accent color the kit offers has a message", () => {
		expect(THEME_PRESETS.length).toBeGreaterThan(0);
		for (const p of THEME_PRESETS)
			expect(Object.hasOwn(ACCENT_MESSAGES, p.id), `no message for accent ${p.id}`).toBe(true);
	});

	test("each accent reads as the kit's label in English", () => {
		for (const p of THEME_PRESETS) expect(accentLabel(p)).toBe(p.label);
	});

	test("an accent this build does not know shows the kit's label, and a name like constructor is not a lookup", () => {
		expect(accentLabel({ id: "teal", label: "Teal" })).toBe("Teal");
		expect(accentLabel({ id: "constructor", label: "Constructor" })).toBe("Constructor");
	});
});

describe("appearanceLabel", () => {
	test("names both appearances", () => {
		expect(appearanceLabel("dark")).toBe("Dark");
		expect(appearanceLabel("light")).toBe("Light");
	});
});
