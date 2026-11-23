// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "./__fixtures__/pty-frames.json";
import { decodeFrame } from "./ptyChannel.ts";
import { ptyStream } from "./ptyStream.ts";

/** The channel delivers each frame as an ArrayBuffer. */
const frame = (hex: string): ArrayBuffer => {
	const b = new Uint8Array(hex.match(/../g)!.map((h) => parseInt(h, 16)));
	return b.buffer;
};

// Contract with `crates/atlas-desktop/src/pty_feed.rs`: `pty-frames.json` holds the frames the REAL Rust feed produced
// (hex), the page's REAL decoder and stream consume them. A kind byte or a layout changed on one side is red here.
describe("frames from the Rust feed", () => {
	test("a history, live output and the end decode to what xterm is given", async () => {
		const got = [];
		const written = [];
		const p = ptyStream({
			open: async (onFrame) => {
				for (const f of fixture.history_then_live_then_exit) onFrame(decodeFrame(frame(f)));
				return { ack: () => {}, close: () => {} };
			},
			attached: async () => {},
			write: (b, replay, done) => (
				got.push(replay),
				written.push(new TextDecoder().decode(b, { stream: true })),
				done()
			),
			reset: () => {},
		});
		await Promise.resolve();
		await Promise.resolve();
		expect(written.join("")).toBe(fixture.history_utf8 + "dir\r\n");
		expect(got[0]).toBe(true);
		p.dispose();
	});

	test("an empty history is still a reset, and the stream says when it was cut", () => {
		const frames = fixture.empty_history_then_dropped.map((f) => decodeFrame(frame(f)));
		expect(frames.map((f) => f.kind)).toEqual(["replay", "data", "dropped"]);
		expect(frames[0].bytes.length).toBe(0);
		expect(new TextDecoder().decode(frames[1].bytes)).toBe("x");
	});

	test("bytes are a view of the frame, not a copy, and multi-byte text survives", () => {
		const f = decodeFrame(frame(fixture.history_then_live_then_exit[0]));
		expect(f.kind).toBe("replay");
		expect(f.bytes.buffer.byteLength).toBeGreaterThan(f.bytes.length);
		expect(new TextDecoder().decode(f.bytes).startsWith(fixture.history_utf8)).toBe(true);
	});

	test("a frame kind this page does not know is an error, not a silent skip", () => {
		expect(() => decodeFrame(frame("09"))).toThrow(/unknown terminal frame kind 9/);
	});
});
