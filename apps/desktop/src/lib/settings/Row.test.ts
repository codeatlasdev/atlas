// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// Incident (Settings ▸ Integrations ▸ Agent hooks): the agent logo sat ~38 px (and up to ~170 px on a wide dialog) before
// the agent's name. `.main` is `justify-content: space-between` and `.text` is capped at 62ch, so once the row was
// wider than lead + 62ch + control the leftover went BETWEEN the logo and the text (the `margin-right: -10px` on the
// logo only hid 10 px of it). The logo and the text are one group: the free space belongs between that group and the
// control. Real component for the structure; the stylesheet is read from the source because happy-dom has no layout
// (the geometry is measured in Atlas Lab, see the PR).
const { flushSync, mount, unmount, createRawSnippet } = await import("svelte");
const { default: Row } = await import("./Row.svelte");

const src = readFileSync(join(import.meta.dir, "Row.svelte"), "utf8");
const css = src.slice(src.indexOf("<style>"));
/** Declarations of the first rule whose selector is exactly `sel`. */
function rule(sel: string): string {
	const m = new RegExp(`(?:^|\\n)\\t${sel.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`).exec(
		css,
	);
	return m?.[1] ?? "";
}

let done;
afterEach(() => (done?.(), (done = undefined)));
const snippet = (html: string) => createRawSnippet(() => ({ render: () => html }));
function render(props) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Row, { target, props }));
	done = () => (unmount(app), target.remove());
	return target;
}

test("the lead and the text are one group, so free space can only land between the group and the control", () => {
	const t = render({
		label: "omp",
		description: "d",
		lead: snippet(`<i class="logo"></i>`),
		children: snippet(`<b class="ctl"></b>`),
	});
	const main = t.querySelector(".main");
	const group = t.querySelector(".ident");
	expect(group).not.toBeNull();
	expect(group.querySelector(".lead .logo")).not.toBeNull();
	expect(group.querySelector(".text .label").textContent).toBe("omp");
	// Direct children of the flex row: the group and the control, nothing between the icon and its name.
	expect([...main.children].map((c) => c.className.split(" ")[0])).toEqual(["ident", "control"]);
});

test("a row without a lead has no empty slot before its text", () => {
	const t = render({ label: "x", children: snippet(`<b></b>`) });
	expect(t.querySelector(".lead")).toBeNull();
});

test("the icon slot hugs its icon: a fixed box, never shrunk, with a standard gap and no negative-margin hack", () => {
	const lead = rule(".lead");
	expect(lead).toMatch(/flex-shrink:\s*0/);
	expect(lead).toMatch(/width:\s*(24|28|32)px/);
	expect(lead).not.toMatch(/margin[^;]*-\d/);
	expect(rule(".ident")).toMatch(/gap:\s*12px/);
});

test("the group takes the free width, so the control stays on the right edge", () => {
	expect(rule(".ident")).toMatch(/flex:\s*1/);
	expect(rule(".ident")).toMatch(/min-width:\s*0/);
	expect(rule(".ident")).toMatch(/align-items:\s*center/);
});

test("the control is centred on the row's text block, not on the first line", () => {
	expect(rule(".main")).toMatch(/align-items:\s*center/);
	expect(rule(".control")).toMatch(/align-items:\s*center/);
});
