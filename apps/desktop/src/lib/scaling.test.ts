// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { growth, linearBound } from "../../test/scaling.ts";

// The editor, redaction and bracket-colour guards rest on `growth` telling a linear pass from a quadratic one on any
// runner. Incident (2026-11-19, PR #631's CI): `trimTrailingWhitespace`, a single linear pass, measured 38.6 against
// the bound of 24 once in 40 runs; locally it is 6.8 to 9.5. The ratio is now the median of three rounds, which must
// not blur the line it exists to draw: a real quadratic still lands far above the bound.

const text = (n: number) => "x".repeat(n);

test("a linear pass grows about as much as its input, under the bound", () => {
	const linear = (n: number) => {
		const t = text(n);
		return () => {
			let s = 0;
			for (let i = 0; i < t.length; i++) s += t.charCodeAt(i);
			return s;
		};
	};
	expect(growth(linear, 16_384)).toBeLessThan(linearBound());
});

test("a quadratic pass (a rescan from every position) is over the bound", () => {
	// The classic regression: for each start, scan to the end (what a backtracking regex or a string rebuilt per
	// character costs). n / 16 starts keep the small size fast while the growth stays quadratic.
	const quadratic = (n: number) => {
		const t = text(n);
		return () => {
			let s = 0;
			for (let start = 0; start < t.length; start += 16)
				for (let i = start; i < t.length; i++) s += t.charCodeAt(i);
			return s;
		};
	};
	// The big input costs ~64x the small one, and `growth` sizes the small sample to `minMs`: at the default 4 ms the
	// 21 big samples ran past bun's 5 s test timeout on every run on an AMD laptop (2026-11-23, 10 of 10). 1 ms keeps
	// the ratio (~64 against a bound of 24): 20 of 20 runs passed there, the whole file in under 5 s.
	expect(growth(quadratic, 2_048, { minMs: 1 })).toBeGreaterThan(linearBound());
});
