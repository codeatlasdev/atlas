// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { expect, test } from "bun:test";
import { parseAnsi, css, stripAnsi, ANSI_VARS } from "./ansi.ts";
const E = "\x1b";
test("vite/turbo line", () => {
	const r = parseAnsi(
		`${E}[36mstore:dev: ${E}[0m ${E}[2m03:18:44${E}[22m ${E}[34m[200]${E}[39m / ${E}[2m29ms${E}[22m`,
	);
	expect(r.map((x) => x.text).join("")).toBe("store:dev:  03:18:44 [200] / 29ms");
	expect(r[0].style?.fg).toBe("var(--ansi-cyan)");
	expect(r.find((x) => x.text === "[200]")?.style?.fg).toBe("var(--ansi-blue)");
	expect(r.find((x) => x.text === "03:18:44")?.style?.dim).toBe(true);
});
test("256, truecolor, OSC link, cursor, CR", () => {
	const r = parseAnsi(
		`old\r${E}]8;;http://x${E}\\link${E}]8;;${E}\\ ${E}[38;5;208mo${E}[38;2;1;2;3mt${E}[2K${E}[1;31mE${E}[0m`,
	);
	expect(r.map((x) => x.text).join("")).toBe("link otE");
	expect(r.find((x) => x.text === "o")?.style?.fg).toBe("rgb(255 135 0)");
	expect(r.find((x) => x.text === "t")?.style?.fg).toBe("rgb(1 2 3)");
	expect(css(r.find((x) => x.text === "E")!.style)).toContain("font-weight");
});
test("plain fast path", () => {
	expect(parseAnsi("hello")).toEqual([{ text: "hello", style: null }]);
});

test("underline styles, underline color, SGR 21 double, 24 resets", () => {
	const r = parseAnsi(`${E}[4:3;58;2;255;0;0mw${E}[24mx${E}[21my${E}[4:0mz${E}[4ms${E}[59mt`);
	expect(r[0].style).toEqual({ underline: "curly", ulColor: "rgb(255 0 0)" });
	// 24 clears the underline but not its color (only 59 does), as in a terminal.
	expect(r[1].style).toEqual({ ulColor: "rgb(255 0 0)" });
	expect(r[2].style?.underline).toBe("double");
	expect(r[3].style).toEqual({ ulColor: "rgb(255 0 0)" });
	expect(r[4].style).toEqual({ underline: "single", ulColor: "rgb(255 0 0)" });
	expect(r[5].style).toEqual({ underline: "single" });
	expect(css(r[0].style)).toContain("text-decoration-style:wavy");
	expect(css(r[0].style)).toContain("text-decoration-color:rgb(255 0 0)");
});

test("underline subparameter does not swallow following params", () => {
	const r = parseAnsi(`${E}[4:2;1;31mx`);
	expect(r[0].style).toMatchObject({ underline: "double", bold: true, fg: "var(--ansi-red)" });
});

test("bright, 256 cube and grayscale, colon-form truecolor, background and dim/italic/strike/inverse", () => {
	const r = parseAnsi(
		`${E}[91ma${E}[0m${E}[38;5;196mb${E}[0m${E}[38;5;240mc${E}[0m${E}[38:2:10:20:30md${E}[0m${E}[48;5;21;2;3;9;7me`,
	);
	expect(r[0].style?.fg).toBe("var(--ansi-bright-red)");
	expect(r[1].style?.fg).toBe("rgb(255 0 0)");
	expect(r[2].style?.fg).toBe("rgb(88 88 88)");
	expect(r[3].style?.fg).toBe("rgb(10 20 30)");
	expect(r[4].style).toMatchObject({
		bg: "rgb(0 0 255)",
		dim: true,
		italic: true,
		strike: true,
		inverse: true,
	});
	const c = css(r[4].style);
	expect(c).toContain("line-through");
	expect(c).toContain("font-style:italic");
});

test("base colors resolve through the --ansi-* variables the theme store writes", () => {
	expect(ANSI_VARS).toHaveLength(16);
	expect(ANSI_VARS[1]).toBe("--ansi-red");
	expect(ANSI_VARS[8]).toBe("--ansi-bright-black");
});

test("OSC 8 hyperlinks attach to the runs they wrap (BEL and ST terminators) and close", () => {
	const r = parseAnsi(
		`a ${E}]8;id=1;https://x.dev/p${E}\\${E}[31mred${E}[0m link${E}]8;;${E}\\ b ${E}]8;;mailto:me@x.dev\x07m${E}]8;;\x07`,
	);
	expect(r.map((x) => x.text).join("")).toBe("a red link b m");
	const linked = r.filter((x) => x.style?.link === "https://x.dev/p");
	expect(linked.map((x) => x.text).join("")).toBe("red link");
	expect(linked[0].style.fg).toBe("var(--ansi-red)");
	expect(r.find((x) => x.text === "m")?.style?.link).toBe("mailto:me@x.dev");
	expect(r.find((x) => x.text === " b ")?.style).toBeNull();
});

test("OSC 8 refuses script-ish schemes", () => {
	const r = parseAnsi(`${E}]8;;javascript:alert(1)${E}\\x${E}]8;;${E}\\`);
	expect(r[0].style?.link).toBeUndefined();
});

test("a reset inside a link keeps the link", () => {
	const r = parseAnsi(`${E}]8;;http://a${E}\\${E}[31mx${E}[0my${E}]8;;${E}\\`);
	expect(r.map((x) => x.style?.link)).toEqual(["http://a", "http://a"]);
});

test("CRLF and carriage-return progress bars keep the final frame", () => {
	expect(
		parseAnsi("done\r")
			.map((x) => x.text)
			.join(""),
	).toBe("done");
	expect(
		parseAnsi("10%\r50%\r100%\r")
			.map((x) => x.text)
			.join(""),
	).toBe("100%");
	expect(
		parseAnsi("10%\r50%\r100%")
			.map((x) => x.text)
			.join(""),
	).toBe("100%");
	expect(stripAnsi(`${E}[32mok${E}[0m\r`)).toBe("ok");
});

test("malformed and truncated escapes never throw or leak control bytes", () => {
	for (const s of [
		`${E}`,
		`${E}[`,
		`${E}[38;5m`,
		`${E}[38;2;1m`,
		`${E}[999mx`,
		`${E}]8;;`,
		`a${E}[?25lb\x07\x08c`,
	]) {
		const text = parseAnsi(s)
			.map((x) => x.text)
			.join("");
		expect(text).not.toMatch(/[\x00-\x08\x0b-\x1f]/);
	}
	expect(
		parseAnsi(`a${E}[?25lb\x07c`)
			.map((x) => x.text)
			.join(""),
	).toBe("abc");
});

test("a pathological run explosion is capped, text preserved", () => {
	const raw = Array.from({ length: 3000 }, (_, i) => `${E}[3${i % 8}mx`).join("");
	const r = parseAnsi(raw);
	expect(r.length).toBeLessThanOrEqual(512);
	expect(r.map((x) => x.text).join("")).toBe("x".repeat(3000));
});

test("stripAnsi matches the text parseAnsi renders", () => {
	const raw = `${E}[36mstore${E}[0m ${E}]8;;http://x${E}\\l${E}]8;;${E}\\ ${E}[2K${E}[1;31mE`;
	expect(stripAnsi(raw)).toBe(
		parseAnsi(raw)
			.map((x) => x.text)
			.join(""),
	);
});
