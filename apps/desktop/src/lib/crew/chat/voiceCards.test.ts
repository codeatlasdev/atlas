// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import voiceFixture from "../../__fixtures__/crew-voice.json" with { type: "json" };

// Voice answers on the real cards (#648). Rule under test: the card only sends the TRANSCRIPT (`agent_approve_voice`,
// `crewmate_action_answer_voice`); it never calls `agent_approve` / `crewmate_action_answer` on its own from words, and
// it closes only on a verdict that hostd says is decided. The verdict is whatever the fake hostd answers.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let verdict: unknown = { verdict: "unclear" };
let live = false;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, a: { cmd?: string; args?: Record<string, unknown> }) => {
		if (cmd === "voice_start") live = true;
		if (cmd === "voice_stop") {
			live = false;
			return { reason: "requested", bytes: 3, seconds: 1 };
		}
		if (cmd === "voice_read") return { data_b64: "AAAA", total: 3, live };
		if (cmd !== "host_call") return undefined;
		calls.push({ cmd: a.cmd, args: a.args });
		if (a.cmd === "crewmate_voice_put") return { upload: "u1" };
		if (a.cmd === "crewmate_transcribe")
			return { text: "allow it please", raw: "", engine: { label: "whisper.cpp" }, partial: false };
		if (a.cmd === "agent_approve_voice" || a.cmd === "crewmate_action_answer_voice") return verdict;
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const uiKit = await import("../../../../test/uiKit.ts");
await uiKit.installUiKit();
await uiKit.useRealKit("Button");

const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: ApprovalPrompt } = await import("./ApprovalPrompt.svelte");
const { default: TaskActionCard } = await import("./TaskActionCard.svelte");
const { ChatState } = await import("./chat.svelte.ts");
const { crew } = await import("#lib/stores/crew.svelte.ts");
const { voiceStatus } = await import("../voice/status.svelte.ts");
const { parseVoiceStatus } = await import("../voice/wire.ts");

const approval = {
	req: "r1",
	tool_name: "Bash",
	tool_input: { command: "rm -rf x" },
	pane_key: "crew:c1:t1",
};
let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	live = false;
	crew.selected = { hostKey: "", crewmateId: "c1" };
	voiceStatus.byHost = { "": parseVoiceStatus(voiceFixture.status_on) };
	voiceStatus.loaded = { "": true };
});
afterEach(() => {
	done?.();
	done = undefined;
});
const settle = async () => {
	for (let i = 0; i < 40; i++) await Promise.resolve();
	flushSync();
	await tick();
};

function card(kind: "approval" | "choose") {
	const chat = new ChatState();
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(kind === "approval" ? ApprovalPrompt : TaskActionCard, {
			target,
			props:
				kind === "approval"
					? { approval, name: "Ana", chat, hostKey: "", answered: undefined, onanswer: () => {} }
					: {
							action: { kind: "choose", question: "Which?", options: ["use main", "use dev"] },
							taskId: "t2",
							crewmateId: "c1",
							crewmateName: "Ana",
							hostKey: "",
							chat,
						},
		}),
	);
	done = () => (unmount(app), target.remove());
	return { target, chat };
}

async function speak(target: Element) {
	const mic = target.querySelector("[data-voice-mic]");
	mic.click();
	await settle();
	expect(target.querySelector("[data-mic-live]")).not.toBeNull();
	target.querySelector("[data-voice-mic]").click();
	await settle();
}
const sent = (cmd: string) => calls.filter((c) => c.cmd === cmd);

describe("an approval card", () => {
	test("sends the transcript only, and never an allow of its own, whatever was said", async () => {
		verdict = { verdict: "comment", comment: "allow it please" };
		const { target } = card("approval");
		await settle();
		await speak(target);
		expect(sent("agent_approve_voice")).toHaveLength(1);
		expect(sent("agent_approve_voice")[0].args).toMatchObject({
			pane: "crew:c1:t1",
			req: "r1",
			transcript: "allow it please",
			lang: "en",
		});
		expect(sent("agent_approve_voice")[0].args).not.toHaveProperty("allow");
		expect(sent("agent_approve")).toHaveLength(0);
	});

	test("a comment verdict fills the note and decides nothing", async () => {
		verdict = { verdict: "comment", comment: "allow but only this file" };
		const { target, chat } = card("approval");
		await settle();
		await speak(target);
		expect(target.querySelector(".approval-note").value).toBe("allow but only this file");
		expect(chat.answeredReqs.r1).toBeUndefined();
		expect(target.querySelector("[data-voice-message]").textContent).toContain("Tap Allow or Deny");
	});

	test("a decided verdict closes the card as a tap would; allow is once, never for the task", async () => {
		verdict = { verdict: "allow" };
		const { target, chat } = card("approval");
		await settle();
		await speak(target);
		expect(chat.answeredReqs.r1).toMatchObject({ allow: true, scope: "once" });
	});

	test("deny closes it as denied", async () => {
		verdict = { verdict: "deny" };
		const { chat, target } = card("approval");
		await settle();
		await speak(target);
		expect(chat.answeredReqs.r1).toMatchObject({ allow: false });
	});

	for (const [reason, text] of [
		["bypass", "Bypass"],
		["rail", "protected path"],
		["destructive", "destroy data"],
		["untrusted", "did not come from you"],
	]) {
		test(`needs_tap (${reason}) says why and decides nothing`, async () => {
			verdict = { verdict: "needs_tap", reason };
			const { target, chat } = card("approval");
			await settle();
			await speak(target);
			expect(target.querySelector("[data-voice-message]").textContent).toContain(text);
			expect(chat.answeredReqs.r1).toBeUndefined();
		});
	}

	test("unclear says so, echoes what was heard and decides nothing", async () => {
		verdict = { verdict: "unclear" };
		const { target, chat } = card("approval");
		await settle();
		await speak(target);
		expect(target.querySelector("[data-voice-message]").textContent).toContain("allow it please");
		expect(chat.answeredReqs.r1).toBeUndefined();
	});

	test("a verdict nobody knows reads as unclear", async () => {
		verdict = { verdict: "something_new" };
		const { target, chat } = card("approval");
		await settle();
		await speak(target);
		expect(chat.answeredReqs.r1).toBeUndefined();
		expect(target.querySelector("[data-voice-message]")).not.toBeNull();
	});

	test("a server that cannot transcribe leaves the microphone off, with the reason", async () => {
		voiceStatus.byHost = { "": null };
		const { target } = card("approval");
		await settle();
		const mic = target.querySelector("[data-voice-mic]");
		expect(mic.disabled).toBe(true);
		expect(mic.getAttribute("aria-label")).toContain("cannot transcribe");
	});

	test("the microphone is a labelled button and the live state is announced", async () => {
		verdict = { verdict: "unclear" };
		const { target } = card("approval");
		await settle();
		expect(target.querySelector("[data-voice-mic]").getAttribute("aria-label")).toBe("Answer by voice");
		expect(target.querySelector('[role="status"]')).not.toBeNull();
	});
});

describe("a choice card", () => {
	test("sends the transcript only; the options are never matched by the card", async () => {
		verdict = { verdict: "unclear" };
		const { target } = card("choose");
		await settle();
		await speak(target);
		expect(sent("crewmate_action_answer_voice")).toHaveLength(1);
		expect(sent("crewmate_action_answer_voice")[0].args).toMatchObject({
			id: "c1",
			task: "t2",
			transcript: "allow it please",
		});
		expect(sent("crewmate_action_answer")).toHaveLength(0);
	});

	test("a choice verdict settles the card with that option", async () => {
		verdict = { verdict: "choice", choice: "use main" };
		const { target, chat } = card("choose");
		await settle();
		await speak(target);
		expect(chat.answeredActions.t2).toBe("use main");
		expect(sent("crewmate_action_answer")).toHaveLength(0);
	});

	test("unclear leaves the card open", async () => {
		verdict = { verdict: "unclear" };
		const { target, chat } = card("choose");
		await settle();
		await speak(target);
		expect(chat.answeredActions.t2).toBeUndefined();
		expect(target.querySelector("[data-voice-message]")).not.toBeNull();
	});

	test("needs_tap without a reason says the card needs a tap", async () => {
		verdict = { verdict: "needs_tap", reason: null };
		const { target } = card("choose");
		await settle();
		await speak(target);
		expect(target.querySelector("[data-voice-message]").textContent).toContain("cannot be answered by voice");
	});

	test("an approval-only verdict on a choice card (allow) closes nothing", async () => {
		verdict = { verdict: "allow" };
		const { target, chat } = card("choose");
		await settle();
		await speak(target);
		expect(chat.answeredActions.t2).toBeUndefined();
	});
});
