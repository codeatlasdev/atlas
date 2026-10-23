// The mirror turns a click on its image into a click in the app; a wrong mapping clicks the wrong control.
import { expect, test } from "bun:test";
import { authorized, toPage } from "./desktop-mirror.ts";

test("a click on the downscaled image lands on the same CSS point of the page", () => {
	// Real numbers: a 1272x782 CSS window, captured at 2x and sent as a 1600 px wide JPEG.
	const img = { w: 1600, h: 983 };
	const view = { w: 1272, h: 782 };
	expect(toPage(img, view, 0, 0)).toEqual({ x: 0, y: 0 });
	expect(toPage(img, view, 1600, 983)).toEqual({ x: 1272, y: 781 });
	// "Skip for now" at CSS (1210, 735) is drawn at image (1522, 925).
	expect(toPage(img, view, 1522, 925)).toEqual({ x: 1210, y: 735 });
});

test("only the run's token opens the mirror", () => {
	const tok = "a".repeat(32);
	expect(authorized(new URL(`http://127.0.0.1:1/stream?t=${tok}`), tok)).toBe(true);
	for (const u of ["/stream", "/stream?t=", `/stream?t=${"b".repeat(32)}`, `/?t=${tok}x`]) {
		expect(authorized(new URL(`http://127.0.0.1:1${u}`), tok)).toBe(false);
	}
});
