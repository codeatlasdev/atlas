// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { beforeEach, describe, expect, mock, test } from "bun:test";

// What goes on the wire for a task whose text was dictated: `crewmate_task` carries `dictated: true` (hostd then tells
// the agent to expect homophones), only when some of the draft was spoken, and a button's answer never does. Real
// `ChatState` and `crew.sendTask`; only the Tauri boundary is stubbed, and it records every host call.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (_cmd: string, a: { cmd: string; args: Record<string, unknown> }) => {
		calls.push({ cmd: a.cmd, args: a.args });
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../../test/uiKit.ts")).installUiKit();

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { ChatState } = await import("./chat.svelte.ts");
const { crew } = await import("#lib/stores/crew.svelte.ts");

const sent = () => calls.filter((c) => c.cmd === "crewmate_task");

beforeEach(() => {
	calls.length = 0;
	crew.selected = { hostKey: "", crewmateId: "crewmate-working" };
});

describe("the dictated flag of a task", () => {
	test("a draft with dictated text goes out with dictated: true, and the flag is spent once it is sent", async () => {
		const chat = new ChatState();
		chat.prompt = "open src/main.rs";
		chat.dictation.dictated = true;
		await chat.send();
		expect(sent()).toHaveLength(1);
		expect(sent()[0].args).toMatchObject({ prompt: "open src/main.rs", dictated: true });
		expect(chat.dictation.dictated).toBe(false);

		chat.prompt = "and then run the tests";
		await chat.send();
		expect(sent()[1].args).not.toHaveProperty("dictated");
	});

	test("a typed draft carries no flag", async () => {
		const chat = new ChatState();
		chat.prompt = "rename x";
		await chat.send();
		expect(sent()[0].args).not.toHaveProperty("dictated");
	});

	test("an answer given by a button (or a retry) is never marked dictated, even while the draft is", async () => {
		const chat = new ChatState();
		chat.dictation.dictated = true;
		await chat.sendText("yes, go ahead");
		expect(sent()[0].args).toMatchObject({ prompt: "yes, go ahead" });
		expect(sent()[0].args).not.toHaveProperty("dictated");
	});

	test("a send that fails keeps the flag with the draft, so the retry still says it was dictated", async () => {
		const chat = new ChatState();
		chat.prompt = "open src/main.rs";
		chat.dictation.dictated = true;
		const real = crew.sendTask;
		crew.sendTask = async () => {
			throw new Error("offline");
		};
		await chat.send();
		crew.sendTask = real;
		expect(chat.prompt).toBe("open src/main.rs");
		expect(chat.dictation.dictated).toBe(true);
	});
});
