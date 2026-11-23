// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";

// #289 (docs/perf/linux-webkit-study.md): a streaming chat spent 36.5 % of a core on a GPU because every delta rebuilt the
// whole message (`{@html marked.parse(text)}`) and re-evaluated the list. The real Transcript over the real ChatState:
// a delta may touch only the DOM of the block that grew. MutationObserver reports what the browser would re-layout.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../../test/uiKit.ts")).useRealKit("Spinner");
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Harness } = await import("../../../../test/TranscriptHarness.svelte");
const { applyBatch } = await import("#lib/rpc.ts");

let done: (() => void) | undefined;
afterEach(() => (done?.(), (done = undefined)));

const msg = (role: string, text: string, id: string) =>
	JSON.stringify({
		type: "message_end",
		messageId: id,
		message: { role, content: [{ type: "text", text }] },
	});
const delta = (text: string) =>
	JSON.stringify({
		type: "message_update",
		assistantMessageEvent: { partial: { role: "assistant", content: [{ type: "text", text }] } },
	});

function transcript() {
	const target = document.createElement("div");
	document.body.append(target);
	let chat: () => any;
	const app = flushSync(() => mount(Harness, { target, props: { expose: (c) => (chat = c) } }));
	done = () => (unmount(app), target.remove());
	return { target, chat: () => chat() };
}
const settle = async () => (flushSync(), await tick());

test("a delta touches only the block that grew: earlier rows and earlier blocks keep their DOM", async () => {
	const { target, chat } = transcript();
	applyBatch(chat(), [
		msg("user", "first question", "u1"),
		msg("assistant", "an old answer", "a1"),
		msg("user", "second question", "u2"),
		delta("# Title\n\nfirst paragraph\n\nsecond par"),
	]);
	await settle();
	const rows = [...target.querySelectorAll(".vl-row")];
	expect(rows).toHaveLength(4);
	const streamed = rows[3].querySelector(".md");
	const blocks = [...streamed.children];
	expect(blocks.map((b) => b.tagName)).toEqual(["H1", "P", "P"]);

	const touched = new Set<Node>();
	const watch = new MutationObserver((rs) => rs.forEach((r) => touched.add(r.target)));
	watch.observe(target, { subtree: true, childList: true, characterData: true, attributes: true });
	// 20 more cumulative deltas arrive in one batch (one frame): the newest text wins.
	const grown = Array.from({ length: 20 }, (_, i) =>
		delta(`# Title\n\nfirst paragraph\n\nsecond par${"x".repeat(i + 1)}`),
	);
	applyBatch(chat(), grown);
	await settle();
	watch.disconnect();

	expect(streamed.textContent).toContain("second par" + "x".repeat(20));
	// Other rows: not one mutation inside them.
	for (const row of rows.slice(0, 3)) expect([...touched].filter((n) => row.contains(n))).toEqual([]);
	// Earlier blocks of the streaming message are the same nodes and were not written to.
	const after = [...streamed.children];
	expect(after[0]).toBe(blocks[0]);
	expect(after[1]).toBe(blocks[1]);
	expect([...touched].filter((n) => blocks[0].contains(n) || blocks[1].contains(n))).toEqual([]);
});
