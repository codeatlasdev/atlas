// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { longestLine, planOpen, planSave, toViewText } from "./session.ts";
import { largeNotice } from "./largeFile.ts";
import { m } from "#lib/i18n/m.ts";
import { DEFAULT_PREFS } from "#lib/prefs.ts";
import { growth, linearBound } from "../../../test/scaling.ts";

const prefs = { ...DEFAULT_PREFS.editor };
const plan = (raw: string, over: Partial<Parameters<typeof planOpen>[0]> = {}) =>
	planOpen({ raw, bytes: raw.length, path: "/w/src/a.ts", files: [], prefs, ...over });

test("CRLF files show as \\n in the view and are written back as CRLF", () => {
	const p = plan("a\r\nb\r\n");
	expect(p.text).toBe("a\nb\n");
	expect(p.eol).toBe("crlf");
	expect(p.mixedEol).toBe(false);
	expect(planSave("a\nb\nc\n", p)).toBe("a\r\nb\r\nc\r\n");
});

test("mixed endings are flagged and normalised to LF on save", () => {
	const p = plan("a\r\nb\nc\r");
	expect(p.mixedEol).toBe(true);
	expect(p.eol).toBe("lf");
	expect(p.text).toBe("a\nb\nc\n");
});

test(".editorconfig beats detection and preferences", () => {
	const files = [
		{
			dir: "/w",
			text: "root=true\n[*]\nindent_style=tab\ntab_width=8\nend_of_line=crlf\ntrim_trailing_whitespace=false\ninsert_final_newline=false\n",
		},
	];
	const p = plan("    spaced();\n    x();\n", { files });
	expect(p.indent).toEqual({ style: "tab", size: 8, detected: false });
	expect(p.eol).toBe("crlf");
	expect(p.save).toEqual({ trim: false, finalNewline: false });
	expect(planSave("x  ", p)).toBe("x  ");
});

test("without config the file's own indentation wins over the preference, and says it was detected", () => {
	const two = plan("function f() {\n  if (x) {\n    y();\n  }\n}\n");
	expect(two.indent).toEqual({ style: "space", size: 2, detected: true });
	const tabs = plan("function f() {\n\tif (x) {\n\t\ty();\n\t}\n}\n");
	expect(tabs.indent.style).toBe("tab");
	// No evidence: the preference, not marked as detected.
	expect(plan("one line\n").indent).toEqual({ style: "space", size: 4, detected: false });
});

test("the save policy comes from preferences when there is no .editorconfig", () => {
	const p = plan("x");
	expect(p.save).toEqual({ trim: true, finalNewline: true });
	expect(planSave("a  \nb", p)).toBe("a\nb\n");
	expect(planSave("", p)).toBe("");
});

test("an untouched file saves back to exactly what was on disk (no phantom dirty state)", () => {
	const disk = "a\nb\n";
	const p = plan(disk);
	expect(planSave(p.text, p)).toBe(disk);
	const crlf = "a\r\nb\r\n";
	const q = plan(crlf);
	expect(planSave(q.text, q)).toBe(crlf);
});

// A minified one-liner (5 MB in the incident): opening it is huge-tier, skips indent sniffing, and costs time linear in its
// size, not quadratic (CPU time at 8x the size over the cost at 1x, test/scaling.ts: ~8 vs ~64, whatever the machine or its
// load; it was `< 500 ms` on 5 MB, which failed under load with nothing regressed). The threshold pref is lowered so the
// small sizes take the same huge-tier path as 5 MB, and a quadratic regression is red in seconds instead of hanging.
test("a minified one-liner is huge, skips indent sniffing, and opens in linear time", () => {
	const small = { prefs: { ...prefs, large_file_threshold_bytes: 1_000 } };
	const open = (n: number) => {
		const raw = `{"a":${"1,".repeat(n / 2)}0}`;
		const p = plan(raw, small);
		expect(p.large.tier).toBe("huge");
		expect(largeNotice(p.large.tier)).toBe(m.workbench_editor_notice_huge());
		expect(p.large.disable.lsp).toBe(true);
		return () => plan(raw, small);
	};
	expect(growth(open, 16_384)).toBeLessThan(linearBound());
});

test("the large threshold preference moves the tier boundary", () => {
	const raw = "x\n".repeat(60_000);
	expect(plan(raw, { prefs: { ...prefs, large_file_threshold_bytes: 1_000_000 } }).large.tier).toBe("normal");
	expect(plan(raw, { prefs: { ...prefs, large_file_threshold_bytes: 100_000 } }).large.tier).toBe("large");
});

test("longestLine and toViewText", () => {
	expect(longestLine("")).toBe(0);
	expect(longestLine("ab\nabcd\nabc")).toBe(4);
	expect(toViewText("a\rb\r\nc")).toBe("a\nb\nc");
	expect(toViewText("plain")).toBe("plain");
});
