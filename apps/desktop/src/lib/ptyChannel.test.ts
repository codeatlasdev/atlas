// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "./__fixtures__/pty-frames.json";
import { decodeFrame, orderedChannel } from "./ptyChannel.ts";
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

// Tauri sends a frame of 1 KiB or more through a fetch (asynchronous) and a smaller one through an eval, so a later
// frame can reach the page first; a terminal fed out of order draws garbage. `orderedChannel` is what the library's
// `Channel` does, kept here so the library class stays out of startup JS.
describe("orderedChannel", () => {
	const setup = () => {
		const got: number[] = [];
		const released: number[] = [];
		let raw;
		const ch = orderedChannel(
			{
				transformCallback: (cb) => ((raw = cb), 42),
				unregisterCallback: (id) => released.push(id),
			},
			(m: number) => got.push(m),
		);
		return {
			ch,
			got,
			released,
			send: (index: number, message?: number) =>
				raw(message === undefined ? { index, end: true } : { index, message }),
		};
	};

	test("messages arrive in the order Rust sent them, whatever order they reached the page", () => {
		const { ch, got, send } = setup();
		expect(ch.id).toBe(42);
		send(2, 20);
		send(1, 10);
		expect(got).toEqual([]);
		send(0, 0);
		expect(got).toEqual([0, 10, 20]);
		send(3, 30);
		expect(got).toEqual([0, 10, 20, 30]);
	});

	test("the callback is released once the end message and everything before it were delivered", () => {
		const { got, released, send } = setup();
		send(1, 1);
		send(2); // end, before message 0 arrived
		expect(released).toEqual([]);
		send(0, 0);
		expect(got).toEqual([0, 1]);
		expect(released).toEqual([42]);
		const b = setup();
		b.send(0, 0);
		b.send(1); // end, in order
		expect(b.released).toEqual([42]);
	});

	test("a muted channel delivers nothing more but keeps the callback until Rust ends it", () => {
		const { ch, got, released, send } = setup();
		send(0, 0);
		ch.mute();
		send(1, 1);
		expect(got).toEqual([0]);
		expect(released).toEqual([]);
		send(2);
		expect(released).toEqual([42]);
	});

	test("a handler that throws does not stall the channel, and the error is still reported", () => {
		const got: number[] = [];
		let raw;
		orderedChannel(
			{ transformCallback: (cb) => ((raw = cb), 1), unregisterCallback: () => {} },
			(m: number) => {
				got.push(m);
				if (m === 0) throw new Error("boom");
			},
		);
		const queued: (() => void)[] = [];
		const real = globalThis.queueMicrotask;
		globalThis.queueMicrotask = (f) => void queued.push(f);
		try {
			raw({ index: 0, message: 0 });
			raw({ index: 1, message: 1 });
		} finally {
			globalThis.queueMicrotask = real;
		}
		expect(got).toEqual([0, 1]);
		expect(queued).toHaveLength(1);
		expect(queued[0]).toThrow("boom");
	});
});
