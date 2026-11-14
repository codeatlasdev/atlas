// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import fixture from "../../__fixtures__/crewmate_task_attachments.json" with { type: "json" };
import {
	CHUNK,
	MAX_ATTACHMENTS,
	MAX_BYTES,
	addImages,
	fetchAttachment,
	imagesFromClipboard,
	sniff,
	toSent,
} from "./attachments.ts";

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]);
const JPG = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0, 0x10]);
const file = (name: string, bytes: Uint8Array, type = "image/png") => new File([bytes], name, { type });

describe("sniff: the type comes from the bytes, not the browser's mime", () => {
	test("each allowed kind", () => {
		expect(sniff(PNG)).toBe("image/png");
		expect(sniff(JPG)).toBe("image/jpeg");
		expect(sniff(new TextEncoder().encode("GIF89a\x01\0"))).toBe("image/gif");
		expect(sniff(new TextEncoder().encode("RIFF\x24\0\0\0WEBPVP8 "))).toBe("image/webp");
	});
	test("everything else is refused", () => {
		expect(sniff(new TextEncoder().encode("<svg xmlns='x'/>"))).toBeNull();
		expect(sniff(new TextEncoder().encode("%PDF-1.7"))).toBeNull();
		expect(sniff(new TextEncoder().encode("RIFF\x24\0\0\0WAVEfmt "))).toBeNull();
		expect(sniff(new Uint8Array([]))).toBeNull();
	});
});

describe("addImages: the composer's pending list", () => {
	test("keeps good images, in order, with their bytes and mime from the bytes", async () => {
		const r = await addImages([], [file("a.png", PNG), file("b.jpg", JPG, "image/png")]);
		expect(r.errors).toEqual([]);
		expect(r.images.map((i) => [i.name, i.mime])).toEqual([
			["a.png", "image/png"],
			["b.jpg", "image/jpeg"],
		]);
		expect(r.images[0].bytes).toEqual(PNG);
	});

	test("a file that is not an image by its bytes is refused with its name, the rest kept", async () => {
		const r = await addImages(
			[],
			[file("shell.png", new TextEncoder().encode("#!/bin/sh\nrm -rf ~")), file("ok.png", PNG)],
		);
		expect(r.images.map((i) => i.name)).toEqual(["ok.png"]);
		expect(r.errors).toHaveLength(1);
		expect(r.errors[0]).toContain("shell.png");
	});

	test("four fit, the fifth is refused, counting what is already pending", async () => {
		const four = await addImages(
			[],
			[1, 2, 3, 4].map((n) => file(`${n}.png`, PNG)),
		);
		expect(four.images).toHaveLength(MAX_ATTACHMENTS);
		const more = await addImages(four.images, [file("5.png", PNG)]);
		expect(more.images).toHaveLength(MAX_ATTACHMENTS);
		expect(more.errors[0]).toContain("at most 4");
	});

	test("5 MiB exactly passes, one byte more is refused", async () => {
		const at = new Uint8Array(MAX_BYTES);
		at.set(PNG);
		expect((await addImages([], [file("at.png", at)])).images).toHaveLength(1);
		const over = new Uint8Array(MAX_BYTES + 1);
		over.set(PNG);
		const r = await addImages([], [file("over.png", over)]);
		expect(r.images).toHaveLength(0);
		expect(r.errors[0]).toContain("over 5 MiB");
	});

	test("an empty file is refused", async () => {
		const r = await addImages([], [file("e.png", new Uint8Array([]))]);
		expect(r.images).toHaveLength(0);
		expect(r.errors).toHaveLength(1);
	});

	test("a pasted screenshot has no name: it gets one", async () => {
		const r = await addImages([], [file("", PNG)]);
		expect(r.images[0].name).toMatch(/^pasted-.*\.png$/);
	});
});

describe("imagesFromClipboard: paste", () => {
	const item = (type: string, f: File | null) => ({ kind: f ? "file" : "string", type, getAsFile: () => f });
	test("takes the image files of a paste and leaves text alone", () => {
		const a = file("", PNG);
		const files = imagesFromClipboard({
			clipboardData: { items: [item("text/plain", null), item("image/png", a)] },
		});
		expect(files).toEqual([a]);
	});
	test("a text-only paste has no images", () => {
		expect(imagesFromClipboard({ clipboardData: { items: [item("text/plain", null)] } })).toEqual([]);
		expect(imagesFromClipboard({ clipboardData: null })).toEqual([]);
	});
});

describe("toSent: what goes on the wire", () => {
	const b64 = (b: Uint8Array) => btoa(String.fromCharCode(...b));
	test("an image that fits one line rides inline in crewmate_task", async () => {
		const calls: unknown[] = [];
		const sent = await toSent(
			[{ name: "a.png", mime: "image/png", bytes: PNG }],
			async (cmd, args) => (calls.push([cmd, args]), {}),
			() => "u",
		);
		expect(calls).toEqual([]);
		expect(sent).toEqual([{ name: "a.png", mime: "image/png", data_b64: b64(PNG) }]);
	});

	test("a big image is uploaded in pieces of at most one chunk, in order, then named by upload id", async () => {
		const big = new Uint8Array(CHUNK * 2 + 5).fill(7);
		big.set(PNG);
		const calls: any[] = [];
		const sent = await toSent(
			[{ name: "big.png", mime: "image/png", bytes: big }],
			async (cmd, args) => (calls.push([cmd, args]), {}),
			() => "up-abcdef123456",
		);
		expect(calls.map(([c]) => c)).toEqual(Array(3).fill("crewmate_attachment_put"));
		expect(calls.map(([, a]) => a.seq)).toEqual([0, 1, 2]);
		expect(calls.map(([, a]) => a.last)).toEqual([false, false, true]);
		expect(calls.every(([, a]) => a.upload === "up-abcdef123456")).toBe(true);
		const total = calls.reduce((n, [, a]) => n + atob(a.data_b64).length, 0);
		expect(total).toBe(big.length);
		for (const [, a] of calls) expect(atob(a.data_b64).length).toBeLessThanOrEqual(CHUNK);
		expect(sent).toEqual([{ name: "big.png", mime: "image/png", upload: "up-abcdef123456" }]);
	});

	test("the extra args every host call carries (host) go with each upload piece", async () => {
		const big = new Uint8Array(CHUNK + 1).fill(1);
		big.set(PNG);
		const calls: any[] = [];
		await toSent(
			[{ name: "b.png", mime: "image/png", bytes: big }],
			async (cmd, args) => (calls.push(args), {}),
			() => "up-abcdef123456",
			{ host: "box", id: "c1" },
		);
		expect(calls.every((a) => a.host === "box" && a.id === "c1")).toBe(true);
	});
});

describe("fetchAttachment: the bytes of a stored image, piece by piece", () => {
	test("reads pieces until `last`, advancing the offset by what arrived", async () => {
		const whole = new Uint8Array(1000).map((_, i) => i % 251);
		const asked: number[] = [];
		const call = async (cmd: string, args: any) => {
			expect(cmd).toBe("crewmate_attachment");
			asked.push(args.offset);
			const end = Math.min(args.offset + 400, whole.length);
			return {
				name: args.name,
				mime: "image/png",
				bytes_len: whole.length,
				offset: args.offset,
				data_b64: btoa(String.fromCharCode(...whole.slice(args.offset, end))),
				last: end >= whole.length,
			};
		};
		const r = await fetchAttachment(call, { id: "c", task: "t", name: "a.png" });
		expect(asked).toEqual([0, 400, 800]);
		expect(r.mime).toBe("image/png");
		expect(r.bytes).toEqual(whole);
	});

	test("an endless server cannot make it read forever", async () => {
		const call = async (_c: string, args: any) => ({
			name: "a.png",
			mime: "image/png",
			bytes_len: 10,
			offset: args.offset,
			data_b64: btoa("xxxxxxxxxx"),
			last: false,
		});
		await expect(fetchAttachment(call, { id: "c", task: "t", name: "a.png" })).rejects.toThrow();
	});
});

describe("contract: the real serializer's task with images", () => {
	test("the keys the thread reads are there, and a task carries records, never bytes", () => {
		const a = fixture.task.attachments;
		expect(a).toHaveLength(2);
		for (const att of a) {
			expect(Object.keys(att).sort()).toEqual(["bytes_len", "mime", "name", "path", "sha256"]);
		}
		expect(fixture.event.task.attachments).toEqual(a);
	});

	test("fetching the real reply of crewmate_attachment gives the file's bytes", async () => {
		const r = await fetchAttachment(async () => fixture.attachment, {
			id: "c",
			task: fixture.task.id,
			name: fixture.task.attachments[0].name,
		});
		expect(r.mime).toBe("image/png");
		expect(r.bytes.length).toBe(fixture.task.attachments[0].bytes_len);
		expect(sniff(r.bytes)).toBe("image/png");
	});
});
