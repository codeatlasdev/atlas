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

// Incident (ui#138, kit 3.1.0): a modal opened over another (Settings → Find projects) put two focus traps on the same
// `focusin`, each pulling focus back from the other: `RangeError: Maximum call stack size exceeded`, 4 → 10 errors per open.
// Kit 4.0.0 gives the focus to the newest trap only. Real kit, two real dialogs, focus moved into the older one.
test("a Dialog opened over another does not overflow the stack and hands focus back when closed", async () => {
	const body = createRawSnippet(() => ({ render: () => `<button class="inner">x</button>` }));
	const Dialog = await realKitComponent("Dialog");
	const host = document.createElement("div");
	document.body.append(host);
	const outer = proxy({ open: true, title: "outer", children: body });
	const inner = proxy({ open: false, title: "inner", children: body });
	const a = flushSync(() => mount(Dialog, { target: host, props: outer }));
	const b = flushSync(() => mount(Dialog, { target: host, props: inner }));
	const errors: unknown[] = [];
	const onError = (e: ErrorEvent) => errors.push(e.error ?? e.message);
	window.addEventListener("error", onError);
	inner.open = true;
	flushSync();
	const [first, second] = [...document.querySelectorAll("dialog")];
	// A focus move inside the older dialog while the newer one is on top: the loop of the incident.
	first.querySelector("button")?.focus();
	second.querySelector("button")?.focus();
	await new Promise((r) => setTimeout(r, 20));
	expect(errors).toEqual([]);
	inner.open = false;
	flushSync();
	await new Promise((r) => setTimeout(r, 20));
	expect(errors).toEqual([]);
	expect(document.querySelectorAll("dialog").length).toBe(1);
	window.removeEventListener("error", onError);
	unmount(b);
	unmount(a);
	host.remove();
});

// Contract the keymap gate (modalOpen.ts, codeatlasdev/atlas#66) reads: the kit announces a modal on `document` as
// `layerchange` with `detail.modal`, true for a Dialog and false for a menu. If a kit upgrade renames or drops it, the
// shortcuts would run behind Settings again, so this goes red first. Real kit, real event.
test("a Dialog announces itself as a modal on document (layerchange) and a menu does not", async () => {
	const seen: boolean[] = [];
	const on = (e: Event) => seen.push(e.detail.modal);
	document.addEventListener("layerchange", on);
	const host = document.createElement("div");
	document.body.append(host);
	const Dialog = await realKitComponent("Dialog");
	const props = proxy({ open: false, title: "t", children: text });
	const app = flushSync(() => mount(Dialog, { target: host, props }));
	props.open = true;
	flushSync();
	expect(seen.at(-1)).toBe(true);
	props.open = false;
	flushSync();
	await new Promise((r) => setTimeout(r, 30));
	expect(seen.at(-1)).toBe(false);
	document.removeEventListener("layerchange", on);
	unmount(app);
	host.remove();
});

// Acceptance of codeatlasdev/atlas#66 ("overlays opened from inside a modal stack above it"). `showModal()` makes
// everything outside the dialog inert, top-layer popovers included, so an overlay that is not INSIDE the modal's subtree
// is dead (unclickable, unfocusable) however high it paints. Atlas declares most of them elsewhere in the tree (a menu
// in the shell, a confirm opened from a Settings button) and opens them while Settings is up: the kit's layer engine
// must move them into the modal. Real kit; the trigger is moved into the open dialog to model "opened from inside it".
// happy-dom has no hit-testing, so containment is the checkable half; ui's own browser suite (ModalContract, ui#225)
// owns the pixel half.
test("a DropdownMenu and a Popover declared outside a Dialog but opened while it is up live inside it", async () => {
	await (
		await import("../../../test/uiKit.ts")
	).useRealKit("Dialog", "DropdownMenu", "DropdownMenu/DropdownMenuItem", "Popover");
	const { default: Harness } = await import("../../../test/DialogOverlaysHarness.svelte");
	const host = document.createElement("div");
	document.body.append(host);
	const app = flushSync(() => mount(Harness, { target: host, props: {} }));
	const dialog = document.querySelector("dialog[role=dialog]");
	expect(dialog).not.toBeNull();
	for (const [sel, layerSel] of [
		[".menu-t", "[role=menu]"],
		[".pop-t", "[popover]:not(dialog)"],
	]) {
		const trigger = host.querySelector(sel);
		expect(trigger).not.toBeNull();
		expect(dialog.contains(trigger)).toBe(false); // declared outside the modal
		trigger.click();
		flushSync();
		await new Promise((r) => setTimeout(r, 20));
		const layer = document.querySelector(layerSel);
		expect(layer).not.toBeNull();
		expect(dialog.contains(layer)).toBe(true);
		trigger.click();
		flushSync();
		await new Promise((r) => setTimeout(r, 30));
	}
	unmount(app);
	host.remove();
});
