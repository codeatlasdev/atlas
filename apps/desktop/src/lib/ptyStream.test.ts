// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { BACKLOG_MAX, droppedPty, ptyStream } from "./ptyStream.ts";

const enc = (s) => new TextEncoder().encode(s);
const dec = (b) => new TextDecoder().decode(b);

/** A host whose `listen` registers only when the test says so; `attach` answers with the replay at once. */
function fakeHost(history = "PS> ") {
	let cb = null;
	let register;
	const registered = new Promise((r) => (register = r));
	const host = {
		attaches: 0,
		emit: (data, replay = false) => cb?.({ data, replay }),
		listen: async (f) => {
			await registered;
			cb = f;
			return () => (cb = null);
		},
		attach: async () => {
			host.attaches++;
			host.emit(history, true);
		},
		register,
	};
	return host;
}

function sink() {
	const out = { text: "", resets: 0 };
	return { out, write: (b) => (out.text += dec(b)), reset: () => ((out.text = ""), out.resets++) };
}

/** Let promises settle: a few microtask turns, no clock. */
const tick = async () => {
	for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe("ptyStream", () => {
	// Incident: terminals on Windows/Linux (xterm.js) opened blank until F5. The component attached (asking hostd
	// for the replay) before its `listen` had registered; a fast host answered first, the replay was lost, and every
	// later byte waited behind a replay that never came.
	test("attaches only once the listener is live, so a fast replay is never lost", async () => {
		const h = fakeHost();
		const s = sink();
		ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...s });
		await tick();
		expect(h.attaches).toBe(0);
		h.register();
		await tick();
		expect(h.attaches).toBe(1);
		h.emit("dir\r\n");
		expect(s.out.text).toBe("PS> dir\r\n");
	});

	test("live output that beats the replay is held and written after it, in order", async () => {
		let cb;
		const s = sink();
		ptyStream({
			listen: async (f) => ((cb = f), () => {}),
			attach: async () => {
				cb({ data: "b", replay: false });
				cb({ data: "a", replay: true });
			},
			decode: enc,
			...s,
		});
		await tick();
		await tick();
		expect(s.out.text).toBe("ab");
	});

	test("reattach (host reconnected) replays from scratch instead of appending to stale screen", async () => {
		const h = fakeHost("v1");
		const s = sink();
		const p = ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...s });
		h.register();
		await tick();
		await tick();
		h.emit("x");
		expect(s.out.text).toBe("v1x");
		p.reattach();
		await tick();
		await tick();
		expect(h.attaches).toBe(2);
		expect(s.out.text).toBe("v1");
		expect(s.out.resets).toBe(2);
	});

	test("dispose before the listener registers unsubscribes it and never attaches", async () => {
		const h = fakeHost();
		const s = sink();
		const p = ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...s });
		p.dispose();
		h.register();
		await tick();
		await tick();
		h.emit("late", true);
		expect(h.attaches).toBe(0);
		expect(s.out.text).toBe("");
	});
});

// hostd stops feeding a client that does not read (a webview paused in the background) and says so with
// `pty_dropped` on `host://event`; the terminal must reattach, whose replay redraws what it missed.
describe("droppedPty", () => {
	test("names the pty only for a pty_dropped event", () => {
		expect(droppedPty({ ev: "pty_dropped", pty: "p1", reason: "slow_client" })).toBe("p1");
		expect(droppedPty({ ev: "pty_exit", pty: "p1" })).toBeUndefined();
		expect(droppedPty({ ev: "pty_dropped" })).toBeUndefined();
		expect(droppedPty({ ev: "pty_dropped", pty: 7 })).toBeUndefined();
		expect(droppedPty(null)).toBeUndefined();
		expect(droppedPty("pty_dropped")).toBeUndefined();
	});

	test("a drop followed by reattach redraws from the replay and keeps streaming", async () => {
		const h = fakeHost("screen@100MB");
		const s = sink();
		const p = ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...s });
		h.register();
		await tick();
		await tick();
		h.emit("old");
		// What the component does on the event:
		if (droppedPty({ ev: "pty_dropped", pty: "p1", reason: "slow_client" }) === "p1") p.reattach();
		await tick();
		await tick();
		expect(s.out.text).toBe("screen@100MB");
		h.emit("+live");
		expect(s.out.text).toBe("screen@100MB+live");
		expect(h.attaches).toBe(2);
	});
});

// Incident (2026-10-26, Linux/Arch: two busy terminals froze the app): `t.write` per event with no flow control.
// xterm.js buffers every write and parses it in 12 ms slices; a producer faster than the webview (a build log, `cat` of
// a big file, an agent redrawing 30 times a second on a slow WebKitGTK) grows that buffer without limit until it
// discards data at 50 MB, and the app is busy parsing for as long as it takes to drain. hostd's own cap never trips,
// because the desktop process reads the socket faster than the webview consumes. The stream now bounds what xterm holds
// and, past the bound, drops the rest and redraws from hostd's replay once xterm has caught up.
describe("backlog", () => {
	/** An xterm whose parser is as slow as the test says: `done` callbacks run only when `drain()` is called. */
	function slowXterm() {
		const q: { n: number; done?: () => void }[] = [];
		const x = {
			written: 0,
			resets: 0,
			write: (b: Uint8Array, _replay?: boolean, done?: () => void) => {
				x.written += b.length;
				q.push({ n: b.length, done });
			},
			reset: () => {
				x.resets++;
				x.written = 0;
				q.length = 0;
			},
			/** xterm parsed everything it was given. */
			drain: () => q.splice(0).forEach((e) => e.done?.()),
		};
		return x;
	}
	const flood = (n: number) => "x".repeat(n);
	const MB = 1024 * 1024;

	async function started(history = "screen") {
		const h = fakeHost(history);
		const x = slowXterm();
		const p = ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...x });
		h.register();
		await tick();
		await tick();
		x.drain();
		return { h, x, p };
	}

	test("output below the bound is written as it arrives, untouched", async () => {
		const { h, x } = await started();
		const before = x.written;
		for (let i = 0; i < 50; i++) h.emit(flood(1000));
		expect(x.written - before).toBe(50_000);
		expect(h.attaches).toBe(1);
	});

	test("past the bound xterm is no longer fed: what it holds stays bounded", async () => {
		const { h, x } = await started();
		for (let i = 0; i < 200; i++) h.emit(flood(MB / 4)); // 50 MiB offered
		expect(x.written).toBeLessThanOrEqual(BACKLOG_MAX + MB / 4);
	});

	test("once xterm has caught up the screen is redrawn from hostd's replay, and live output follows it", async () => {
		const { h, x } = await started("screen");
		for (let i = 0; i < 200; i++) h.emit(flood(MB / 4));
		expect(h.attaches).toBe(1); // not before it caught up: the replay would queue behind the backlog
		x.drain();
		await tick();
		await tick();
		expect(h.attaches).toBe(2);
		expect(x.resets).toBe(2);
		expect(x.written).toBe("screen".length);
		h.emit("+live");
		expect(x.written).toBe("screen+live".length);
	});

	test("a flood is one resync, not one per dropped chunk", async () => {
		const { h, x } = await started();
		for (let i = 0; i < 400; i++) h.emit(flood(MB / 4));
		x.drain();
		x.drain();
		await tick();
		await tick();
		expect(h.attaches).toBe(2);
	});

	test("a second flood after the resync is bounded and resynced again", async () => {
		const { h, x } = await started();
		for (let i = 0; i < 100; i++) h.emit(flood(MB / 4));
		x.drain();
		await tick();
		await tick();
		for (let i = 0; i < 100; i++) h.emit(flood(MB / 4));
		expect(x.written).toBeLessThanOrEqual(BACKLOG_MAX + MB / 4 + "screen".length);
		x.drain();
		await tick();
		await tick();
		expect(h.attaches).toBe(3);
	});

	test("a replay from the host while catching up is not lost: it is the resync", async () => {
		const { h, x, p } = await started("v1");
		for (let i = 0; i < 100; i++) h.emit(flood(MB / 4));
		p.reattach(); // e.g. host://reconnected during the flood
		await tick();
		await tick();
		x.drain();
		await tick();
		await tick();
		expect(x.written).toBe("v1".length);
	});

	// xterm still calls the `done` of writes that were queued before a reset; counting them against the fresh buffer would
	// push the backlog negative and let the next flood through unbounded.
	test("a late `done` from before a reset does not corrupt the count", async () => {
		const h = fakeHost("v1");
		const late: (() => void)[] = [];
		let written = 0;
		const p = ptyStream({
			listen: h.listen,
			attach: h.attach,
			decode: enc,
			reset: () => {},
			write: (b, _r, done) => {
				written += b.length;
				if (done) late.push(done);
			},
		});
		h.register();
		await tick();
		await tick();
		const before = late.slice();
		p.reattach(); // a second replay: reset
		await tick();
		await tick();
		before.forEach((d) => d()); // the first replay's callback fires after the reset
		written = 0;
		for (let i = 0; i < 100; i++) h.emit(flood(MB / 4));
		expect(written).toBeLessThanOrEqual(BACKLOG_MAX + MB / 4);
	});

	test("dispose while catching up never reattaches", async () => {
		const { h, x, p } = await started();
		for (let i = 0; i < 100; i++) h.emit(flood(MB / 4));
		p.dispose();
		x.drain();
		await tick();
		await tick();
		expect(h.attaches).toBe(1);
	});

	test("a sink that never calls `done` (a fake with no callback) still works below the bound", async () => {
		const h = fakeHost("hi");
		const s = sink();
		ptyStream({ listen: h.listen, attach: h.attach, decode: enc, ...s });
		h.register();
		await tick();
		await tick();
		h.emit("there");
		expect(s.out.text).toBe("hithere");
	});
});
