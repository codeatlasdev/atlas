// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import fixture from "../../../__fixtures__/crewmate_task_attachments.json" with { type: "json" };

// The images of a task in the thread, against the real `TaskImages`, the real `hostCall` and the real piece-by-piece
// fetch. Only the Tauri boundary is stubbed: `host_call` answers `crewmate_attachment` with what the Rust
// serializer wrote (`crewmate_task_attachments.json`), so a rename of a key on either side fails here.
const calls: { cmd: string; args: Record<string, unknown> }[] = [];
let failing = false;
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (_cmd: string, a: { cmd: string; args: Record<string, unknown> }) => {
		calls.push({ cmd: a.cmd, args: a.args });
		if (failing) throw new Error("offline");
		return fixture.attachment;
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
await (await import("../../../../test/uiKit.ts")).installUiKit();

// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: TaskImages } = await import("./TaskImages.svelte");
const { clearAttachmentCache } = await import("../attachmentCache.ts");

// happy-dom has no object URLs.
const created: string[] = [];
const revoked: string[] = [];
URL.createObjectURL = (b: Blob) => {
	const u = `blob:test/${created.length}-${b.type}-${b.size}`;
	created.push(u);
	return u;
};
URL.revokeObjectURL = (u: string) => void revoked.push(u);

let done: (() => void) | undefined;
beforeEach(() => {
	calls.length = 0;
	created.length = 0;
	revoked.length = 0;
	failing = false;
	clearAttachmentCache();
});
afterEach(() => {
	done?.();
	done = undefined;
});

/** Let pending promises and Svelte flushes run until the document stops changing: the condition, not a guessed delay.
 *  The stubbed `invoke` answers at once, so the fetch chain is a bounded number of microtasks per piece. */
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

function setup(props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(TaskImages, {
			target,
			props: {
				attachments: [fixture.task.attachments[0]],
				taskId: fixture.task.id,
				crewmateId: "crewmate-working",
				hostKey: "",
				...props,
			},
		}),
	);
	done = () => (unmount(app), target.remove());
	return target;
}

describe("the thread's images", () => {
	test("an attachment is fetched from the crew server and shown, named in its alt text", async () => {
		const target = setup();
		await settle();
		const img = target.querySelector("img");
		expect(img).not.toBeNull();
		expect(img.alt).toBe(fixture.task.attachments[0].name);
		expect(img.src).toContain("blob:test/");
		expect(created[0]).toContain("image/png");
		// The request is the contract hostd serves: which crewmate, task and image, from the start.
		expect(calls).toHaveLength(1);
		expect(calls[0].cmd).toBe("crewmate_attachment");
	});

	test("the host call names the crewmate, the task, the image and the crew server", async () => {
		const seen: Record<string, unknown>[] = [];
		const target = setup({ hostKey: "ssh:crew@box" });
		await settle();
		seen.push(...calls.map((c) => c.args));
		expect(seen[0]).toMatchObject({
			id: "crewmate-working",
			task: fixture.task.id,
			name: fixture.task.attachments[0].name,
			offset: 0,
			host: "ssh:crew@box",
		});
		expect(target.querySelector("img")).not.toBeNull();
	});

	test("a local crewmate sends no host key", async () => {
		setup();
		await settle();
		expect("host" in calls[0].args).toBe(false);
	});

	test("two images: both shown, one fetch each", async () => {
		const target = setup({ attachments: fixture.task.attachments });
		await settle();
		expect(target.querySelectorAll("img")).toHaveLength(2);
		expect(calls).toHaveLength(2);
	});

	test("rendering the same task again does not fetch again", async () => {
		setup();
		await settle();
		done();
		setup();
		await settle();
		expect(calls).toHaveLength(1);
	});

	test("a failed fetch says so on that image and does not break the thread", async () => {
		failing = true;
		const target = setup();
		await settle();
		expect(target.querySelector("img")).toBeNull();
		const failed = target.querySelector("[role=img][aria-label$='could not be loaded']");
		expect(failed).not.toBeNull();
		expect(failed.getAttribute("aria-label")).toContain(fixture.task.attachments[0].name);
	});

	test("while it loads there is a placeholder, not an empty hole", async () => {
		const target = setup();
		expect(target.querySelector("[role=img][aria-label^='Loading']")).not.toBeNull();
		await settle();
		expect(target.querySelector("[aria-label^='Loading']")).toBeNull();
	});

	test("a click opens the image large and a click on it closes it", async () => {
		const target = setup();
		await settle();
		target
			.querySelector("button[aria-label^='Open']")
			.dispatchEvent(new MouseEvent("click", { bubbles: true }));
		await settle();
		const box = target.querySelector("button.lightbox");
		expect(box).not.toBeNull();
		expect(target.querySelectorAll("img")).toHaveLength(2);
		box.dispatchEvent(new MouseEvent("click", { bubbles: true }));
		await settle();
		expect(target.querySelector("button.lightbox")).toBeNull();
	});

	test("the object URL is revoked when the thumbnail goes away", async () => {
		setup();
		await settle();
		expect(created.length).toBeGreaterThan(0);
		done();
		done = undefined;
		await settle();
		for (const u of created) expect(revoked).toContain(u);
	});
});
