// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { beforeEach, describe, expect, mock, test } from "bun:test";
import voiceFixture from "../../__fixtures__/crew-voice.json" with { type: "json" };

// "Read replies aloud" in the chat (#648): the toggle is per device and off by default, turning it on asks the
// crewmate for `spoken: true`, a reply that finishes is read (only `task.spoken`), and a dictation that starts silences
// the voice. Real `ChatState`, `Speech`, `Dictation`, `crew` store; only the Tauri boundary is stubbed and recorded.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
const raw: string[] = [];
const said: { text: string; lang: string }[] = [];
let refuse: string | null = null;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, a: { cmd: string; args: Record<string, unknown> }) => {
		if (cmd !== "host_call") {
			raw.push(cmd);
			if (cmd === "speech_available") return true;
			if (cmd === "speech_speak") {
				said.push(a as never);
				return 7;
			}
			return undefined;
		}
		calls.push({ cmd: a.cmd, args: a.args });
		if (a.cmd === "crewmate_update" && refuse) throw { code: "crew.forbidden", message: refuse };
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../../test/uiKit.ts")).installUiKit();

const { ChatState } = await import("./chat.svelte.ts");
const { crew } = await import("#lib/stores/crew.svelte.ts");
const { readAloud } = await import("../voice/readAloud.svelte.ts");
const { parseVoiceStatus } = await import("../voice/wire.ts");
const { voiceStatus } = await import("../voice/status.svelte.ts");
const { crewmateKey } = await import("../crewmateKey.ts");

const KEY = crewmateKey("", "crewmate-working");
const task = (id: string, state: string, extra = {}) => ({
	id,
	state,
	ts: 1,
	prompt: "p",
	source: "desktop",
	...extra,
});
const updates = () => calls.filter((c) => c.cmd === "crewmate_update");
const spoken = () => raw.filter((c) => c === "speech_speak");

beforeEach(() => {
	calls.length = 0;
	raw.length = 0;
	said.length = 0;
	refuse = null;
	localStorage.clear();
	readAloud.set(KEY, false);
	crew.selected = { hostKey: "", crewmateId: "crewmate-working" };
});

describe("the toggle", () => {
	test("is off by default for every crewmate on this device", () => {
		expect(new ChatState().readsAloud()).toBe(false);
	});

	test("turning it on sets spoken_replies on the crewmate and is remembered on this device", async () => {
		const chat = new ChatState();
		await chat.setReadAloud(true);
		expect(updates()).toHaveLength(1);
		expect(updates()[0].args).toMatchObject({ id: "crewmate-working", spoken_replies: true });
		expect(chat.readsAloud()).toBe(true);
		expect(JSON.parse(localStorage.getItem("atlas.readAloud"))).toEqual([KEY]);
	});

	test("turning it off silences this device and leaves the crewmate's flag alone", async () => {
		const chat = new ChatState();
		await chat.setReadAloud(true);
		calls.length = 0;
		raw.length = 0;
		await chat.setReadAloud(false);
		expect(chat.readsAloud()).toBe(false);
		expect(updates()).toHaveLength(0);
		expect(raw).toContain("speech_stop");
	});

	test("a crewmate that refuses the flag leaves the toggle off: no reading without summaries", async () => {
		refuse = "only the owner";
		const chat = new ChatState();
		await chat.setReadAloud(true);
		expect(chat.readsAloud()).toBe(false);
	});
});

describe("a reply that finishes", () => {
	async function opened(on = true) {
		const chat = new ChatState();
		if (on) await chat.setReadAloud(true);
		await chat.speech.attach(new EventTarget());
		chat.onTasks([task("t1", "running")], false);
		return chat;
	}

	test("is read aloud with hostd's `spoken`, never the visible reply", async () => {
		const chat = await opened();
		chat.onTasks(
			[task("t1", "done", { reply: "A long markdown reply with `code`.", spoken: "I fixed the bug." })],
			false,
		);
		for (let i = 0; i < 10; i++) await Promise.resolve();
		expect(said).toEqual([{ text: "I fixed the bug.", lang: "en" }]);
		expect(chat.speech.speaking).toBe(true);
	});

	test("nothing is spoken when `spoken` is empty", async () => {
		const chat = await opened();
		chat.onTasks([task("t1", "done", { reply: "```code only```", spoken: "" })], false);
		await Promise.resolve();
		expect(spoken()).toHaveLength(0);
	});

	test("nothing is spoken while the toggle is off, even when hostd sends `spoken`", async () => {
		const chat = await opened(false);
		chat.onTasks([task("t1", "done", { spoken: "I fixed the bug." })], false);
		await Promise.resolve();
		expect(spoken()).toHaveLength(0);
	});

	test("tasks that are still loading (maybe another crewmate's) are neither read nor taken as the baseline", async () => {
		const chat = new ChatState();
		await chat.setReadAloud(true);
		await chat.speech.attach(new EventTarget());
		chat.onTasks([task("other", "done", { spoken: "Not this chat." })], true);
		chat.onTasks([task("t1", "done", { spoken: "Old." })], false);
		await Promise.resolve();
		expect(spoken()).toHaveLength(0);
	});
});

describe("the microphone", () => {
	test("starting a dictation silences the reply being read before the microphone opens", async () => {
		voiceStatus.byHost = { "": parseVoiceStatus(voiceFixture.status_on) };
		const chat = new ChatState();
		await chat.speech.attach(new EventTarget());
		await chat.speech.speak("A reply being read.");
		expect(chat.speech.speaking).toBe(true);
		raw.length = 0;
		chat.dictation.toggle({ read: () => ({ value: "", start: 0, end: 0 }), write() {} });
		for (let i = 0; i < 20; i++) await Promise.resolve();
		expect(chat.speech.speaking).toBe(false);
		expect(raw.indexOf("speech_stop")).toBeGreaterThanOrEqual(0);
		expect(raw.indexOf("speech_stop")).toBeLessThan(raw.indexOf("voice_start"));
	});
});
