// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import { afterEach, describe, expect, test } from "bun:test";

// The composer's image intake against the real component: the "+" opens a file input, a pasted screenshot is
// taken instead of inserted as text, and the pending images show with a remove button. The component only
// reports files; `addImages` (attachments.test.ts) decides what is accepted.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load
// (a static import is hoisted above it and cannot compile a .svelte file).
const { flushSync, mount, unmount, tick } = await import("svelte");
const { default: Composer } = await import("./Composer.svelte");

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]);
const png = (name = "a.png") => new File([PNG], name, { type: "image/png" });

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
});

function setup(props = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const calls = { files: [] as File[][], removed: [] as number[], sent: 0 };
	const app = flushSync(() =>
		mount(Composer, {
			target,
			props: {
				value: "",
				placeholder: "Message",
				label: "Task prompt",
				onsend: () => calls.sent++,
				onfiles: (f: File[]) => calls.files.push(f),
				onremoveimage: (i: number) => calls.removed.push(i),
				...props,
			},
		}),
	);
	done = () => (unmount(app), target.remove());
	return { target, calls };
}
const settle = async () => (flushSync(), await tick());
const attachBtn = (t: Element) => t.querySelector("button[aria-label=Attach]");
const picker = (t: Element) => t.querySelector("input[type=file]") as HTMLInputElement;
const box = (t: Element) => t.querySelector("textarea") as HTMLTextAreaElement;

function paste(el: Element, items: { kind: string; type: string; file?: File }[]) {
	const e = new Event("paste", { bubbles: true, cancelable: true });
	e.clipboardData = { items: items.map((i) => ({ ...i, getAsFile: () => i.file ?? null })) };
	el.dispatchEvent(e);
	return e;
}

describe("the + and the picker", () => {
	test("the + opens the file input, which takes the four image types and several files", () => {
		const { target } = setup();
		let clicked = 0;
		picker(target).click = () => clicked++;
		attachBtn(target).dispatchEvent(new MouseEvent("click", { bubbles: true }));
		expect(clicked).toBe(1);
		const input = picker(target);
		expect(input.accept).toBe("image/png,image/jpeg,image/webp,image/gif");
		expect(input.multiple).toBe(true);
	});

	test("files picked are handed over, and the same file can be picked again", async () => {
		const { target, calls } = setup();
		const input = picker(target);
		Object.defineProperty(input, "files", { value: [png("a.png"), png("b.png")], configurable: true });
		input.dispatchEvent(new Event("change", { bubbles: true }));
		await settle();
		expect(calls.files).toHaveLength(1);
		expect(calls.files[0].map((f) => f.name)).toEqual(["a.png", "b.png"]);
		// Reset so choosing the same file again fires `change` (a browser does not fire it for an unchanged value).
		expect(input.value).toBe("");
	});

	test("without onfiles there is no + at all: the setup conversation keeps its plain box", () => {
		const { target } = setup({ onfiles: undefined });
		expect(attachBtn(target)).toBeNull();
		expect(picker(target)).toBeNull();
	});

	test("with four images pending the + is disabled", async () => {
		const images = [1, 2, 3, 4].map((n) => ({ name: `${n}.png`, mime: "image/png", bytes: PNG }));
		const { target } = setup({ images });
		await settle();
		expect(attachBtn(target).disabled).toBe(true);
	});
});

describe("paste", () => {
	test("a pasted screenshot is taken and not inserted as text", () => {
		const { target, calls } = setup();
		const shot = png("");
		const e = paste(box(target), [{ kind: "file", type: "image/png", file: shot }]);
		expect(e.defaultPrevented).toBe(true);
		expect(calls.files).toEqual([[shot]]);
	});

	test("a text paste is left alone", () => {
		const { target, calls } = setup();
		const e = paste(box(target), [{ kind: "string", type: "text/plain" }]);
		expect(e.defaultPrevented).toBe(false);
		expect(calls.files).toEqual([]);
	});

	test("text and an image together: the image is taken", () => {
		const { target, calls } = setup();
		const e = paste(box(target), [
			{ kind: "string", type: "text/plain" },
			{ kind: "file", type: "image/png", file: png("") },
		]);
		expect(e.defaultPrevented).toBe(true);
		expect(calls.files).toHaveLength(1);
	});

	test("without onfiles a pasted image is not taken over", () => {
		const { target, calls } = setup({ onfiles: undefined });
		const e = paste(box(target), [{ kind: "file", type: "image/png", file: png("") }]);
		expect(e.defaultPrevented).toBe(false);
		expect(calls.files).toEqual([]);
	});
});

describe("pending images", () => {
	const images = [
		{ name: "a.png", mime: "image/png", bytes: PNG },
		{ name: "b.png", mime: "image/png", bytes: PNG },
	];

	test("each shows with a remove button that names it, and removing reports its index", async () => {
		const { target, calls } = setup({ images });
		await settle();
		expect(target.querySelectorAll(".pending-item")).toHaveLength(2);
		target
			.querySelector("button[aria-label='Remove b.png']")
			.dispatchEvent(new MouseEvent("click", { bubbles: true }));
		expect(calls.removed).toEqual([1]);
	});

	test("none pending: no strip", async () => {
		const { target } = setup();
		await settle();
		expect(target.querySelector(".pending")).toBeNull();
	});
});
