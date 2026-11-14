// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { beforeEach, describe, expect, test } from "bun:test";
import { cachedAttachment, clearAttachmentCache } from "./attachmentCache.ts";

const reply = (name: string) => ({
	name,
	mime: "image/png",
	bytes_len: 3,
	offset: 0,
	data_b64: btoa("abc"),
	last: true,
});

beforeEach(clearAttachmentCache);

describe("cachedAttachment", () => {
	test("one fetch per image however many times the thread renders it", async () => {
		let calls = 0;
		const call = async (_c: string, args: { name: string }) => (calls++, reply(args.name));
		const mk = () => cachedAttachment("k", () => ({ id: "c", task: "t", name: "a.png" }), call);
		const [a, b] = await Promise.all([mk(), mk()]);
		await mk();
		expect(calls).toBe(1);
		expect(a).toBe(b);
		expect(new TextDecoder().decode(a.bytes)).toBe("abc");
	});

	test("different keys fetch separately", async () => {
		let calls = 0;
		const call = async (_c: string, args: { name: string }) => (calls++, reply(args.name));
		await cachedAttachment("k1", () => ({ id: "c", task: "t", name: "a.png" }), call);
		await cachedAttachment("k2", () => ({ id: "c", task: "t", name: "b.png" }), call);
		expect(calls).toBe(2);
	});

	test("a failed fetch is not cached: the next render tries again", async () => {
		let calls = 0;
		const call = async (_c: string, args: { name: string }) => {
			calls++;
			if (calls === 1) throw new Error("offline");
			return reply(args.name);
		};
		const mk = () => cachedAttachment("k", () => ({ id: "c", task: "t", name: "a.png" }), call);
		await expect(mk()).rejects.toThrow("offline");
		expect((await mk()).mime).toBe("image/png");
		expect(calls).toBe(2);
	});

	test("the cache holds at most a bounded number of images, dropping the oldest", async () => {
		const call = async (_c: string, args: { name: string }) => reply(args.name);
		let fetched = 0;
		const counting = async (c: string, a: { name: string }) => (fetched++, call(c, a));
		for (let i = 0; i < 40; i++)
			await cachedAttachment(`k${i}`, () => ({ id: "c", task: "t", name: `${i}` }), counting);
		expect(fetched).toBe(40);
		// The newest is still cached, the oldest was let go.
		await cachedAttachment("k39", () => ({ id: "c", task: "t", name: "39" }), counting);
		expect(fetched).toBe(40);
		await cachedAttachment("k0", () => ({ id: "c", task: "t", name: "0" }), counting);
		expect(fetched).toBe(41);
	});
});
