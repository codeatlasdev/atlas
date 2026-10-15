// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";

// Incident (selection stall, 13 spaces; codeatlasdev/ui#27): on kit 0.7.1 every overlay wrapped a bits-ui Portal that
// mounted an imperative root under <body> even while closed (~11 nodes each, ~160 per space switch), so Atlas deferred
// the overlays of neighbour layers (`Overlay`, `AfterIdle`, `Tip`'s lazy tooltip). Kit 2.2 renders a layer only while it
// is `present`, so a closed one is free. This runs the REAL kit (not a stub of it): if a kit upgrade makes a closed
// overlay cost body nodes again, the deferral machinery is needed again and this goes red.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
const { flushSync, mount, unmount, createRawSnippet } = await import("svelte");
const { proxy } = await import("svelte/internal/client");
const { realKitComponent } = await import("../../../test/uiKit.ts");

const text = createRawSnippet(() => ({ render: () => `<span>x</span>` }));
const control = createRawSnippet(() => ({ render: () => `<button class="t">t</button>` }));

const kinds = {
	DropdownMenu: { triggerChild: control, children: text },
	Popover: { triggerChild: control, children: text },
	Tooltip: { content: "x", triggerChild: control },
	Select: { options: [{ value: "a", label: "A" }], value: "a" },
	Dialog: { open: false, title: "t", children: text },
	Drawer: { open: false, title: "t", children: text },
};
const LAYERS = "dialog, [popover], [role=menu], [role=listbox], [role=tooltip]";

for (const [name, props] of Object.entries(kinds)) {
	test(`a closed ${name} adds nothing under <body> beyond its own mount point`, async () => {
		const target = document.createElement("div");
		document.body.append(target);
		const before = document.body.childNodes.length;
		const Component = await realKitComponent(name);
		const app = flushSync(() => mount(Component, { target, props }));
		expect(document.body.childNodes.length).toBe(before);
		expect(document.querySelectorAll(LAYERS).length).toBe(0);
		unmount(app);
		target.remove();
	});
}

test("opening a Dialog is what puts it in the document", async () => {
	const target = document.createElement("div");
	document.body.append(target);
	const props = proxy({ open: false, title: "t" });
	const Dialog = await realKitComponent("Dialog");
	const app = flushSync(() => mount(Dialog, { target, props }));
	expect(document.querySelector("dialog")).toBeNull();
	props.open = true;
	flushSync();
	expect(document.querySelector("dialog[role=dialog]")).not.toBeNull();
	unmount(app);
	target.remove();
});
