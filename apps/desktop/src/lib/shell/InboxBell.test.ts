// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";

// Incident (#506): the notifications popover of an empty inbox said only "No notifications". A first user learns
// nothing from it (what lands here? where is it set?). Real kit Popover and Badge, the real component.
await (await import("../../../test/uiKit.ts")).useRealKit("Popover", "Badge");
const { flushSync, mount, unmount } = await import("svelte");
const { default: InboxBell } = await import("./InboxBell.svelte");

let done;
afterEach(() => {
	done?.();
	document.body.innerHTML = "";
});

const calls = [];
async function open(items) {
	calls.length = 0;
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() =>
		mount(InboxBell, {
			target,
			props: {
				items,
				onRead: (id) => calls.push(["read", id]),
				onReadAll: () => calls.push(["all"]),
				onClear: () => calls.push(["clear"]),
				onSettings: () => calls.push(["settings"]),
			},
		}),
	);
	done = () => unmount(app);
	target.querySelector("button.bell").click();
	flushSync();
	await new Promise((r) => setTimeout(r, 20));
	return target;
}
const text = () => document.body.textContent.replace(/\s+/g, " ");
const button = (label) =>
	[...document.body.querySelectorAll("button")].find((b) => b.textContent.trim() === label);

test("an empty inbox says what lands here and links to the notification settings", async () => {
	await open([]);
	const t = text();
	expect(t).toContain("Agents waiting for your answer");
	expect(t).toContain("tasks that finished");
	expect(t).toContain("updates");
	expect(t).not.toContain("No notifications");
	// Nothing to mark or clear while it is empty.
	expect(button("Mark all read")).toBeUndefined();
	expect(button("Clear")).toBeUndefined();
	button("Notification settings").click();
	flushSync();
	expect(calls).toEqual([["settings"]]);
});

test("with entries it lists them, counts the unread ones, and keeps the settings link out of the way", async () => {
	const target = await open([
		{ id: "a", title: "w · claude", body: "Needs you", read: false, variant: "warning" },
		{ id: "b", title: "w · omp", body: "Done", read: true, variant: "success" },
	]);
	expect(target.querySelector("button.bell").getAttribute("aria-label")).toBe(
		m.shell_chrome_inbox_bell_unread({ count: 1 }),
	);
	expect(text()).toContain("w · claude");
	expect(text()).toContain("w · omp");
	expect(button("Notification settings")).toBeUndefined();
	// Reading a read entry does nothing; an unread one is marked.
	document.body.querySelectorAll(".item")[1].click();
	document.body.querySelectorAll(".item")[0].click();
	button("Mark all read").click();
	button("Clear").click();
	expect(calls).toEqual([["read", "a"], ["all"], ["clear"]]);
});

test("an all-read inbox has no Mark all read, and the bell is named without a count", async () => {
	const target = await open([{ id: "b", title: "w · omp", body: "Done", read: true }]);
	expect(button("Mark all read")).toBeUndefined();
	expect(target.querySelector("button.bell").getAttribute("aria-label")).toBe("Notifications");
});
