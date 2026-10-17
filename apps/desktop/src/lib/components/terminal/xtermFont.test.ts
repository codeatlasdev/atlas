// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { withMonospaceFallback, usableFamilies } from "./xtermFont.ts";

// Incident (#68, xterm path): a user-installed font that WKWebView cannot draw (not loaded yet, or not exposed to the
// web process) fell back to the default serif (Times) for the glyphs while xterm measured the cell on a different
// face, so every character sat in a huge cell ("W e l c o m e  t o  f i s h").

/**
 * A browser stand-in with the numbers WKWebView gave on the incident machine (canvas 16px, `"<family>", serif`):
 * Menlo 9.63/9.63, "SF Mono" 4.45/15.10 (the serif fallback: the app's own default first family is not drawable by
 * the web process), a missing font the same as "SF Mono". `document.fonts.check` is `true` for ALL of them, so
 * it is not part of the contract: only the measured advance tells a drawable fixed-pitch family apart.
 */
function fakeEnv(advance: Record<string, { i: number; W: number }>) {
	return {
		measure: (family: string, ch: string) => advance[family]?.[ch] ?? 4.45 + (ch === "W" ? 10.65 : 0),
	};
}

test("the family list always ends with the generic monospace, even when the user's list has none", () => {
	expect(withMonospaceFallback("Fira Code")).toBe("Fira Code, monospace");
	expect(withMonospaceFallback('"JetBrains Mono", Menlo')).toBe('"JetBrains Mono", Menlo, monospace');
});

test("a list that already ends with monospace is left alone", () => {
	expect(withMonospaceFallback("SF Mono, Menlo, ui-monospace, monospace")).toBe(
		"SF Mono, Menlo, ui-monospace, monospace",
	);
	expect(withMonospaceFallback("Menlo, Monospace")).toBe("Menlo, Monospace");
});

test("a generic that is not last still gets monospace appended (ui-monospace alone is not a safe end)", () => {
	expect(withMonospaceFallback("Menlo, ui-monospace")).toBe("Menlo, ui-monospace, monospace");
	expect(withMonospaceFallback("monospace, Menlo")).toBe("monospace, Menlo, monospace");
});

test("an empty or blank list is just monospace", () => {
	expect(withMonospaceFallback("")).toBe("monospace");
	expect(withMonospaceFallback("  , ")).toBe("monospace");
});

test("the app's default list on WKWebView: SF Mono falls back to serif, so it is dropped and Menlo draws", () => {
	const env = fakeEnv({ Menlo: { i: 9.63, W: 9.63 } });
	expect(usableFamilies("SF Mono, Menlo, ui-monospace, monospace", env)).toBe(
		"Menlo, ui-monospace, monospace",
	);
});

test("a family the webview cannot draw (it measures as the serif fallback) is dropped", () => {
	const env = fakeEnv({ Menlo: { i: 7, W: 7 } });
	expect(usableFamilies("Fira Code, Menlo, monospace", env)).toBe("Menlo, monospace");
});

test("a family that measures nothing (no canvas) is dropped", () => {
	expect(usableFamilies("Nope, monospace", { measure: () => 0 })).toBe("monospace");
});

test("a proportional family is dropped", () => {
	const env = fakeEnv({ Weird: { i: 3, W: 11 } });
	expect(usableFamilies("Weird, monospace", env)).toBe("monospace");
});

test("a fixed-pitch family is kept within a sub-pixel tolerance", () => {
	const env = fakeEnv({ Mono: { i: 7.0, W: 7.4 } });
	expect(usableFamilies("Mono, monospace", env)).toBe("Mono, monospace");
});

test("generic keywords and unmeasurable names pass through untouched: only real families are tested", () => {
	const env = fakeEnv({});
	expect(usableFamilies("ui-monospace, monospace", env)).toBe("ui-monospace, monospace");
});

test("quoted names with commas are one candidate", () => {
	const env = fakeEnv({ "Foo, Bar": { i: 8, W: 8 } });
	expect(usableFamilies('"Foo, Bar", monospace', env)).toBe('"Foo, Bar", monospace');
});

test("when no candidate is usable the result is monospace, never an empty list", () => {
	const env = fakeEnv({});
	expect(usableFamilies("Nope, Also Nope", env)).toBe("monospace");
});
