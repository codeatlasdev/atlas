// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";

// The project tile reads the theme store (Vite-only `import.meta.glob`); the palette only needs it to exist.
mock.module(
	"#lib/shell/ProjectIcon.svelte",
	async () => await import("../../../test/ProjectIconStub.svelte"),
);
await (await import("../../../test/uiKit.ts")).installUiKit();
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: Palette, PALETTE_LIMIT } = await import("./Palette.svelte");

let done;
afterEach(() => done?.());

const item = (label, description) => ({
	id: `${description ?? ""}/${label}`,
	group: "Files",
	label,
	description,
	run: () => {},
});

async function openWith(props, query) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Palette, { target, props: { open: true, ...props } }));
	done = () => (unmount(app), target.remove());
	await tick();
	await tick();
	const input = target.ownerDocument.querySelector("input");
	input.value = query;
	input.dispatchEvent(new Event("input", { bubbles: true }));
	flushSync();
	await tick();
	return [...document.querySelectorAll(".row .label")].map((l) => l.textContent);
}

test("ranked items are shown as given: not filtered, re-sorted or cut by a second scoring", async () => {
	// fuzzy() would put `aaa.ts` (a name match) first: the order here is the index's and must survive.
	const ranked = (q) => [item("zzz.rs", "a/b"), item("aaa.ts"), item("main.ts", "src")];
	const rows = await openWith({ items: ranked, ranked: true }, "a");
	expect(rows).toEqual(["zzz.rs", "aaa.ts", "main.ts"]);
});

test("without `ranked` the palette still scores, orders and caps what it is given", async () => {
	const many = Array.from({ length: 300 }, (_, i) => item(`file${i}.ts`));
	many.push(item("a-better-match.ts"), item("nomatch.md"));
	const rows = await openWith({ items: () => many }, "fi");
	expect(rows).toHaveLength(PALETTE_LIMIT);
	expect(rows).not.toContain("nomatch.md");
	expect(rows[0]).toBe("file0.ts");
});
