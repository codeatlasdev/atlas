// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { growth, linearBound } from "../../../test/scaling.ts";
import { redactText } from "./report.ts";

// A crash report carries whatever the app logged: a base64 blob, a minified bundle line, a data URL. `redactText`
// runs on all of it in the page, so a rule that restarts at every character of one long run freezes the report
// dialog (100k characters took 1.5 s, and 17 s with separators). The check is the growth of the cost with the size
// of the run (CPU time at 8x over 1x, test/scaling.ts), not a wall-clock budget.
describe("redactText scales linearly with a long run", () => {
	// One entry per way a run of token characters can look: no separators (word characters only, the `scheme://` rule),
	// separators at every other character (the `\b` of the named-secret and token rules), and repeated rule prefixes.
	const RUNS = {
		"letters only": (n) => "x".repeat(n),
		"letters and digits": (n) => "a1".repeat(n / 2),
		underscores: (n) => "x_".repeat(n / 2),
		hyphens: (n) => "a-".repeat(n / 2),
		"hyphens, letters and digits": (n) => "a1-".repeat(n / 3),
		"equals signs": (n) => "x=".repeat(n / 2),
		pluses: (n) => "x+".repeat(n / 2),
		slashes: (n) => "a/".repeat(n / 2),
		base64: (n) => "aB3/".repeat(n / 4),
		"keyword name": (n) => "token-".repeat(n / 6),
		"query secrets": (n) => "?token".repeat(n / 6),
		"token prefixes": (n) => "ghp_".repeat(n / 4),
		schemes: (n) => "a://".repeat(n / 4),
		"home paths": (n) => "/Users/".repeat(n / 7),
	};
	for (const [name, run] of Object.entries(RUNS)) {
		test(`${name}: 8x the run costs about 8x`, () => {
			expect(
				growth((n) => {
					const s = run(n);
					return () => redactText(s);
				}, 4800),
			).toBeLessThan(linearBound());
		});
	}
});

describe("redactText on long runs", () => {
	const TOKEN = `ghp_${"A1b2".repeat(9)}`;

	test("a digit at the very end of a 100k run still marks the run as a token", () => {
		expect(redactText(`${"x".repeat(100_000)}1`)).toBe("[redacted]");
		expect(redactText(`${"a-".repeat(50_000)}9`)).toBe("[redacted]");
	});

	test("a run that is all token characters is redacted when it mixes letters and digits, kept when it does not", () => {
		expect(redactText("aB3/".repeat(25_000))).toBe("[redacted]");
		for (const plain of ["x".repeat(100_000), "7".repeat(100_000), "-".repeat(1000), "=".repeat(1000)]) {
			expect(redactText(plain)).toBe(plain);
		}
	});

	test("a secret after a long run is found, and the run is not collateral", () => {
		const run = "a-".repeat(50_000);
		expect(redactText(`${run} ${TOKEN}`)).toBe(`${run} [redacted]`);
		const word = "x".repeat(100_000);
		expect(redactText(`${word} password=hunter2`)).toBe(`${word} password=[redacted]`);
		expect(redactText(`${word} https://ana:pw@github.com/x.git`)).toBe(
			`${word} https://[redacted]@github.com/x.git`,
		);
	});

	test("a secret glued to the end of a run is redacted with it", () => {
		expect(redactText(`${"x".repeat(1000)}/${TOKEN}`)).toBe("[redacted]");
		expect(redactText(`${"x".repeat(100_000)}${TOKEN}`)).toBe("[redacted]");
	});

	test("a named secret inside a long hyphenated name is found", () => {
		const prefix = "a-".repeat(50_000);
		expect(redactText(`${prefix}token=hunter2 next`)).toBe(`${prefix}token=[redacted] next`);
		expect(redactText(`${prefix}-api-key: s3cr3t`)).toBe(`${prefix}-api-key: [redacted]`);
	});

	test("a query secret after many query-looking pieces is found", () => {
		expect(redactText(`${"?token".repeat(5000)}?token=abc&x=1`)).toContain("?token=[redacted]&x=1");
	});
});

describe("redactText token shapes", () => {
	const JWT =
		"eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
	const GH = `ghp_${"A1b2".repeat(9)}`;

	test("a JWT loses all three segments", () => {
		expect(redactText(`got ${JWT}`)).toBe("got [redacted].[redacted].[redacted]");
		expect(redactText(`Authorization header ${JWT} end`)).toBe(
			"Authorization header [redacted].[redacted].[redacted] end",
		);
	});

	test("an AWS access key is redacted bare and under its variable", () => {
		expect(redactText("key AKIAIOSFODNN7EXAMPLE here")).toBe("key [redacted] here");
		expect(redactText("AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE")).toBe("AWS_ACCESS_KEY_ID=[redacted]");
	});

	test("a GitHub token in a URL: credentials, query and path", () => {
		expect(redactText(`clone https://x-access-token:${GH}@github.com/o/r.git`)).toBe(
			"clone https://[redacted]@github.com/o/r.git",
		);
		expect(redactText(`https://example.com/cb?token=${GH}&x=1`)).toBe(
			"https://example.com/cb?token=[redacted]&x=1",
		);
		expect(redactText(`https://example.com/x/github_pat_11ABCDEFG0abcdefghijkl_mnop/y`)).not.toContain(
			"github_pat_",
		);
		expect(redactText(`GET https://api.github.com/repos/o/r/tarball/${GH} 200`)).not.toContain(GH);
	});

	test("a prefixed token after a separator is redacted, a short look-alike is not", () => {
		expect(redactText("x.sk-abcdefgh12345678 and sk-short")).toBe("x.[redacted] and sk-short");
		expect(redactText("a1".repeat(15) + "a")).toBe("a1".repeat(15) + "a");
		expect(redactText(`${"a1".repeat(20)} ok ${"b2".repeat(20)}`)).toBe("[redacted] ok [redacted]");
	});
});
