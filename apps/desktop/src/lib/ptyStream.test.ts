// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import { ptyStream } from "./ptyStream.ts";

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
