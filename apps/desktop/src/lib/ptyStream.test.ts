// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { droppedPty, ptyStream } from "./ptyStream.ts";

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

const tick = () => new Promise((r) => setTimeout(r, 0));

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
