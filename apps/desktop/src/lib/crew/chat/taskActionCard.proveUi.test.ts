// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import fixture from "../../../__fixtures__/crewmate_task_prove_ui.json" with { type: "json" };
import attachmentsFixture from "../../../__fixtures__/crewmate_task_attachments.json" with { type: "json" };

// The action card of a task that needs the owner (#210, #212), against the real `TaskActionCard`, `TaskImages`,
// `chat.svelte.ts` and `hostCall`. Only the Tauri boundary is stubbed: `host_call` answers `crewmate_attachment`
// with what the Rust serializer wrote and records every other call, so the answer the card sends is the wire
// shape hostd parses. The task is `crewmate_task_prove_ui.json`, the real serialization of a parked ui task.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (_cmd: string, a: { cmd: string; args: Record<string, unknown> }) => {
		calls.push({ cmd: a.cmd, args: a.args });
		if (a.cmd === "crewmate_attachment") return attachmentsFixture.attachment;
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
const uiKit = await import("../../../../test/uiKit.ts");
await uiKit.installUiKit();
await uiKit.useRealKit("Button");

const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: TaskActionCard } = await import("./TaskActionCard.svelte");
const { ChatState } = await import("./chat.svelte.ts");
const { clearAttachmentCache } = await import("../attachmentCache.ts");

URL.createObjectURL = () => "blob:test/x";
URL.revokeObjectURL = () => {};

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	clearAttachmentCache();
});
afterEach(() => {
	done?.();
	done = undefined;
});

const settle = async () => {
	let last = "";
	for (let i = 0; i < 50; i++) {
		for (let j = 0; j < 10; j++) await Promise.resolve();
		flushSync();
		await tick();
		const now = document.body.innerHTML;
		if (now === last) return;
		last = now;
	}
};

function setup(task = fixture.task, chat = new ChatState()) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(TaskActionCard, {
			target,
			props: {
				action: task.action,
				taskId: task.id,
				crewmateId: "crewmate-working",
				crewmateName: "Ana",
				hostKey: "",
				attachments: task.attachments,
				chat,
			},
		}),
	);
	done = () => (unmount(app), target.remove());
	return { target, chat };
}

const button = (t: Element, text: string) =>
	[...t.querySelectorAll("button")].find((b) => b.textContent.trim().includes(text));

describe("the prove_ui card", () => {
	test("says it could not prove it, with the reason hostd wrote", async () => {
		const { target } = setup();
		await settle();
		expect(target.textContent).toContain("could not prove");
		expect(target.textContent).toContain(fixture.task.action.reason);
	});

	test("shows the agent's captures, not the owner's own image", async () => {
		const { target } = setup();
		await settle();
		const imgs = [...target.querySelectorAll("img")];
		expect(imgs.map((i) => i.alt)).toEqual(["chromium-390-light.png"]);
		// Fetched from the crew server by the name hostd recorded.
		const fetched = calls.filter((c) => c.cmd === "crewmate_attachment");
		expect(fetched).toHaveLength(1);
		expect(fetched[0].args).toMatchObject({ task: fixture.task.id, name: "chromium-390-light.png" });
	});

	test("Override and Retry answer with the words hostd takes", async () => {
		const { target } = setup();
		await settle();
		button(target, "Override").dispatchEvent(new MouseEvent("click", { bubbles: true }));
		await settle();
		const answer = calls.find((c) => c.cmd === "crewmate_action_answer");
		expect(answer.args).toMatchObject({ id: "crewmate-working", task: fixture.task.id, value: "override" });
		expect(answer.args.attachments ?? []).toEqual([]);
	});

	test("a retry carries the screenshot the owner added, as a task's images go (inline bytes)", async () => {
		const chat = new ChatState();
		const { target } = setup(fixture.task, chat);
		const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]);
		await chat.attachToAction(fixture.task.id, [new File([PNG], "marked.png", { type: "image/png" })]);
		await settle();
		// The pending image is shown on the card with a remove button.
		expect(target.querySelector("[aria-label='Remove marked.png']")).not.toBeNull();
		button(target, "Retry").dispatchEvent(new MouseEvent("click", { bubbles: true }));
		await settle();
		const answer = calls.find((c) => c.cmd === "crewmate_action_answer");
		expect(answer.args.value).toBe("retry");
		expect(answer.args.attachments).toHaveLength(1);
		expect(answer.args.attachments[0]).toMatchObject({ name: "marked.png", mime: "image/png" });
		expect(typeof answer.args.attachments[0].data_b64).toBe("string");
		// The images went with the answer: the card forgets them.
		expect(chat.actionImages[fixture.task.id] ?? []).toEqual([]);
	});

	test("images never ride on an override: the card offers them only with Retry", async () => {
		const chat = new ChatState();
		const { target } = setup(fixture.task, chat);
		const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]);
		await chat.attachToAction(fixture.task.id, [new File([PNG], "marked.png", { type: "image/png" })]);
		await settle();
		button(target, "Override").dispatchEvent(new MouseEvent("click", { bubbles: true }));
		await settle();
		const answer = calls.find((c) => c.cmd === "crewmate_action_answer");
		expect(answer.args.value).toBe("override");
		expect(answer.args.attachments ?? []).toEqual([]);
	});

	test("a file that is not an image is refused on the card, not queued", async () => {
		const chat = new ChatState();
		setup(fixture.task, chat);
		await chat.attachToAction(fixture.task.id, [new File([new Uint8Array([0x4d, 0x5a])], "x.png")]);
		expect(chat.actionImages[fixture.task.id] ?? []).toEqual([]);
	});
});

describe("the refused card shows what it is about", () => {
	test("its reasons, and the agent's images when it has taken some", async () => {
		const task = {
			...fixture.task,
			action: { kind: "refused", reasons: ["removes 1 assertion(s) from src/tests.rs"] },
		};
		const { target } = setup(task);
		await settle();
		expect(target.textContent).toContain("removes 1 assertion(s) from src/tests.rs");
		expect(target.querySelectorAll("img")).toHaveLength(1);
	});

	test("without agent images there is no empty gallery", async () => {
		const task = {
			...fixture.task,
			attachments: fixture.task.attachments.filter((a) => a.source !== "agent"),
		};
		const { target } = setup(task);
		await settle();
		expect(target.querySelector("ul[aria-label]")).toBeNull();
	});
});
