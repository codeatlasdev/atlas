// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { droppedPty, ptyStream } from "./ptyStream.ts";

const enc = (s) => new TextEncoder().encode(s);
const dec = (b) => new TextDecoder().decode(b);

/** A host: `open` registers the frame handler only when the test says so, like a Tauri invoke that is still pending. */
function fakeHost(history = "PS> ") {
	const host = {
		opens: 0,
		closed: 0,
		acked: [],
		onFrame: null,
		emit: (f) => host.onFrame?.(f),
		data: (s) => host.emit({ kind: "data", bytes: enc(s) }),
		replay: (s) => host.emit({ kind: "replay", bytes: enc(s) }),
		open: async (onFrame) => {
			host.opens++;
			host.onFrame = onFrame;
			onFrame({ kind: "replay", bytes: enc(history) });
			return { ack: (n) => host.acked.push(n), close: () => host.closed++ };
		},
	};
	return host;
}

/** xterm: text so far, resets, and `done` callbacks that run only when the test says it parsed. */
function xterm() {
	const x = {
		text: "",
		resets: 0,
		queue: [],
		/** bytes handed to xterm and not parsed yet */
		pending: 0,
		write: (b, _replay, done) => {
			x.text += dec(b);
			x.pending += b.length;
			x.queue.push(() => ((x.pending -= b.length), done()));
		},
		reset: () => {
			x.text = "";
			x.resets++;
		},
		/** xterm parsed everything it was given. */
		parse: () => x.queue.splice(0).forEach((d) => d()),
	};
	return x;
}

/** Let promises settle: a few microtask turns, no clock. */
const tick = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

const start = async (history) => {
	const h = fakeHost(history);
	const x = xterm();
	const p = ptyStream({ open: h.open, attached: async () => {}, write: x.write, reset: x.reset });
	await tick();
	return { h, x, p };
};

describe("ptyStream", () => {
	test("history resets the view, live output follows it in order", async () => {
		const { h, x } = await start("PS> ");
		h.data("dir\r\n");
		h.data("a b\r\n");
		expect(x.text).toBe("PS> dir\r\na b\r\n");
		expect(x.resets).toBe(1);
	});

	// Flow control: Rust sends at most a window of bytes the page has not reported parsed. The page must report
	// exactly what xterm parsed, and only once it did: acking at receipt would defeat the bound, acking more would
	// widen it.
	test("acks the bytes xterm parsed, once it parsed them, and nothing before", async () => {
		const { h, x } = await start("screen");
		h.data("12345");
		h.data("678");
		expect(h.acked).toEqual([]);
		x.parse();
		expect(h.acked).toEqual(["screen".length, 5, 3]);
	});

	// The first frames can reach the page before the attach's answer (the stream id): their acks must not be lost,
	// or that much of Rust's window stays used and a long-running terminal eventually stalls.
	test("bytes parsed before the attach answered are acked once it does", async () => {
		let answer;
		const acked = [];
		const x = xterm();
		ptyStream({
			open: (onFrame) => {
				onFrame({ kind: "replay", bytes: enc("abc") });
				onFrame({ kind: "data", bytes: enc("de") });
				return new Promise((r) => (answer = r));
			},
			attached: async () => {},
			write: x.write,
			reset: x.reset,
		});
		x.parse();
		expect(acked).toEqual([]);
		answer({ ack: (n) => acked.push(n), close: () => {} });
		await tick();
		expect(acked).toEqual([5]);
	});

	test("a parse that finishes after a reattach is not credited to the new stream", async () => {
		const { h, x, p } = await start("v1");
		h.data("stale");
		p.reattach();
		await tick();
		// xterm still parses what it held before the reset, and calls back for it: nothing of that is credited to the
		// new stream, and only then is the new stream opened.
		x.parse();
		await tick();
		expect(h.opens).toBe(2);
		x.parse();
		expect(h.acked).toEqual(["v1".length]); // the new replay only
	});

	// hostd cut this reader (it did not read for 2 s) or Rust gave up on a page that never acknowledged: redraw from
	// the history, never carry on with a hole in the output.
	test("a dropped stream is closed and redrawn from a fresh replay, then keeps streaming", async () => {
		const { h, x } = await start("screen@100MB");
		h.data("old");
		x.parse();
		h.emit({ kind: "dropped" });
		await tick();
		expect(h.opens).toBe(2);
		expect(h.closed).toBe(1);
		expect(x.text).toBe("screen@100MB");
		expect(x.resets).toBe(2);
		h.data("+live");
		expect(x.text).toBe("screen@100MB+live");
	});

	// Incident (load test, 2026-11-22): a page starved for seconds under a flood had hostd cut its stream; each new
	// stream got a fresh window while xterm still held the old backlog (reset() does not drop queued writes), cycle
	// after cycle, until xterm discarded data ("write data discarded, use flow control", 50 MB): the replay's callback
	// never came, the view stayed hidden and nothing was acknowledged again.
	test("a reattach waits until xterm has parsed what it holds, so a slow page never piles up streams", async () => {
		const { h, x } = await start("screen");
		const MB = 1024 * 1024;
		for (let cycle = 0; cycle < 20; cycle++) {
			h.emit({ kind: "data", bytes: new Uint8Array(2 * MB) });
			h.emit({ kind: "dropped" });
			await tick();
		}
		// xterm parsed nothing the whole time: no new stream may have been opened on top of its backlog.
		expect(h.opens).toBe(1);
		expect(x.pending).toBeLessThanOrEqual(2 * MB + "screen".length);
		x.parse();
		await tick();
		expect(h.opens).toBe(2);
		expect(x.text).toBe("screen");
	});

	test("a reconnect while xterm holds unparsed output waits too, then redraws from the replay", async () => {
		const { h, x, p } = await start("v1");
		h.data("backlog");
		p.reattach();
		await tick();
		expect(h.opens).toBe(1);
		expect(h.closed).toBe(1); // the old stream stops feeding at once
		x.parse();
		await tick();
		expect(h.opens).toBe(2);
		expect(x.text).toBe("v1");
	});

	test("an ended program keeps its last screen and is not reattached", async () => {
		const { h, x } = await start("bye");
		h.emit({ kind: "ended" });
		await tick();
		expect(h.opens).toBe(1);
		expect(x.text).toBe("bye");
	});

	test("reattach (host reconnected) replays from scratch instead of appending to a stale screen", async () => {
		const { h, x, p } = await start("v1");
		h.data("x");
		x.parse();
		p.reattach();
		await tick();
		expect(x.text).toBe("v1");
		expect(x.resets).toBe(2);
		expect(h.closed).toBe(1);
	});

	test("dispose closes the stream, and frames that still arrive write nothing", async () => {
		const { h, x, p } = await start("v1");
		p.dispose();
		h.data("late");
		expect(h.closed).toBe(1);
		expect(x.text).toBe("v1");
	});

	test("dispose before the attach answers closes the stream the moment it opens", async () => {
		let answer;
		const link = { ack: () => {}, close: () => link.closed++, closed: 0 };
		const x = xterm();
		const p = ptyStream({
			open: () => new Promise((r) => (answer = r)),
			attached: async () => {},
			write: x.write,
			reset: x.reset,
		});
		p.dispose();
		answer(link);
		await tick();
		expect(link.closed).toBe(1);
	});

	test("an attach the host refuses leaves the view blank and does not throw", async () => {
		const x = xterm();
		ptyStream({
			open: () => Promise.reject(new Error("no such pty")),
			attached: async () => {},
			write: x.write,
			reset: x.reset,
		});
		await tick();
		expect(x.text).toBe("");
	});
});

// hostd stops feeding a client that does not read and says so with `pty_dropped` on `host://event` (the JSON
// listener of the app's own connection); the terminal reattaches, whose replay redraws what it missed.
describe("droppedPty", () => {
	test("names the pty only for a pty_dropped event", () => {
		expect(droppedPty({ ev: "pty_dropped", pty: "p1", reason: "slow_client" })).toBe("p1");
		expect(droppedPty({ ev: "pty_exit", pty: "p1" })).toBeUndefined();
		expect(droppedPty({ ev: "pty_dropped" })).toBeUndefined();
		expect(droppedPty({ ev: "pty_dropped", pty: 7 })).toBeUndefined();
		expect(droppedPty(null)).toBeUndefined();
		expect(droppedPty("pty_dropped")).toBeUndefined();
	});
});
