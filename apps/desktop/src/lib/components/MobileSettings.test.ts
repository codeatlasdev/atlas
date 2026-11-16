// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import devicesFixture from "../__fixtures__/mobile-devices.json";
import pairStartFixture from "../__fixtures__/mobile-pair-start.json";

// Incident (maintainer screenshot): Settings → Mobile listed two phones, both "Atlas Mobile" (the name the app sends,
// the same for every phone), each with five stacked switches and a "Revoke" floating beside the third one; a code could
// expire with nothing but a muted line, and a phone that had just paired only showed up after a reload. The pane is
// the real component over the real replies of hostd (fixtures written by atlas-host `mobile::tests`); only the IPC is
// a fake that answers like hostd does.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
type Args = Record<string, unknown>;
const calls: { cmd: string; args: Args }[] = [];
let devices: Args[] = [];
let pairError: string | null = null;
/** Resolvers waiting for the pane's next `mobile_devices` read: the pane polls, the test awaits that signal. */
let nextRead: (() => void)[] = [];
mock.module("@tauri-apps/api/core", () => ({
	invoke: async (cmd: string, a: { cmd: string; args: Args }) => {
		if (cmd !== "host_call") return null;
		calls.push({ cmd: a.cmd, args: a.args });
		if (a.cmd === "mobile_devices") {
			const reply = { devices };
			nextRead.splice(0).forEach((r) => r());
			return reply;
		}
		if (a.cmd === "mobile_pair_start") {
			if (pairError) throw pairError;
			return pairStartFixture;
		}
		if (a.cmd === "mobile_activity") return { activity: [] };
		return {};
	},
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (
	await import("../../../test/uiKit.ts")
).useRealKit(
	"Button",
	"Dialog",
	"EmptyState",
	"TextField",
	"Switch",
	"Progress",
	"Spinner",
	"CopyButton",
	"Alert",
	"Collapsible",
	"SettingItem",
);
const { flushSync, mount, tick, unmount } = await import("svelte");
const { default: MobileSettings } = await import("./MobileSettings.svelte");
const { default: PairCard } = await import("./mobile/PairCard.svelte");

const [real] = devicesFixture;
const phone = (id: string, over: Args = {}) => ({ ...real, node_id: id, name: "Atlas Mobile", ...over });

let done;
afterEach(() => {
	done?.();
	done = undefined;
	calls.length = 0;
	devices = [];
	pairError = null;
	nextRead = [];
	document.body.innerHTML = "";
});

async function render(Component: unknown, props: Args = {}) {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(Component, { target, props }));
	done = () => (unmount(app), target.remove());
	for (let i = 0; i < 5; i++) await tick();
	flushSync();
	return target;
}
const button = (root: ParentNode, text: string) =>
	[...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
/** Resolves when the pane reads the phone list again (its poll, every second while a code is live). */
const nextDevicesRead = () => {
	const { promise, resolve } = Promise.withResolvers<void>();
	nextRead.push(resolve);
	return promise;
};
async function settle() {
	for (let i = 0; i < 5; i++) await tick();
	flushSync();
}

test("no phones: the pane says what pairing gives and points at the pair card, which has the three steps", async () => {
	const t = await render(MobileSettings);
	const empty = t.querySelector('[data-setting="mobile-devices"]');
	expect(empty.textContent).toContain("No phones paired yet");
	expect(empty.textContent).toContain("restart services");
	expect(empty.textContent).toContain("Show pairing code");
	const steps = [...t.querySelectorAll(".steps strong")].map((s) => s.textContent);
	expect(steps).toEqual(["Get Atlas Mobile", "Open it", "Scan the code"]);
	// Until a store listing exists the app is said to be in testing and the docs page is the link.
	expect(t.querySelector(".steps a").getAttribute("href")).toBe(
		"https://atlas.codeatlas.com.br/docs/mobile/",
	);
	expect(t.querySelector('[data-state="idle"]')).not.toBeNull();
});

test("two phones that both sent 'Atlas Mobile' are told apart; their actions are in the phone's header", async () => {
	devices = [
		phone("a3f9c2", { connected: true, scopes: ["read", "act", "approve", "term_read", "term_write"] }),
		phone("7c1de0", { last_seen: Math.floor(Date.now() / 1000) - 180 }),
	];
	const t = await render(MobileSettings);
	const rows = [...t.querySelectorAll(".row")].filter((r) => r.querySelector(".label"));
	const titles = rows.map((r) => r.querySelector(".label").textContent);
	expect(titles).toEqual(["Phone A3F9", "Phone 7C1D"]);
	expect(titles.join()).not.toContain("Atlas Mobile");
	expect(rows[0].querySelector(".desc").textContent).toContain("Connected now");
	expect(rows[1].querySelector(".desc").textContent).toContain("Last seen 3 min ago");
	expect(rows[0].textContent).toContain("Full access");
	expect(rows[1].textContent).toContain("View only");
	// Rename / Activity / Unpair sit in the header control (beside the name), not under the permissions.
	for (const r of rows) {
		const header = [...r.querySelectorAll(".control button")].map((b) => b.textContent.trim());
		expect(header).toEqual(["Rename", "Activity", "Unpair"]);
		expect(r.querySelector(".below").textContent).not.toContain("Unpair");
	}
	// The five switches are folded away under "Edit permissions", in View and Control groups.
	expect(rows[0].querySelectorAll('input[role="switch"]').length).toBe(5);
	expect(rows[0].querySelector(".ca-collapsible").getAttribute("data-state")).toBe("closed");
	expect([...rows[0].querySelectorAll(".perms h4")].map((h) => h.textContent)).toEqual(["View", "Control"]);
});

test("a permission toggled in the list is sent as scopes in canonical order", async () => {
	devices = [phone("a3f9c2", { scopes: ["read", "term_read"] })];
	const t = await render(MobileSettings);
	const sw = [...t.querySelectorAll('input[role="switch"]')];
	const act = sw[2]; // View: read, term_read; Control: act, term_write, approve
	act.click();
	await settle();
	const set = calls.find((c) => c.cmd === "mobile_set_scopes");
	expect(set.args).toEqual({ node_id: "a3f9c2", scopes: ["read", "act", "term_read"] });
});

test("Unpair asks first, and only the confirmation revokes", async () => {
	devices = [phone("a3f9c2"), phone("7c1de0")];
	const t = await render(MobileSettings);
	const unpair = [...t.querySelectorAll(".control button")].filter(
		(b) => b.textContent.trim() === "Unpair",
	)[1];
	unpair.click();
	await settle();
	expect(calls.some((c) => c.cmd === "mobile_revoke")).toBe(false);
	const dialog = document.querySelector("dialog");
	expect(dialog.textContent).toContain("Unpair Phone 7C1D?");
	button(dialog, "Cancel").click();
	await settle();
	expect(calls.some((c) => c.cmd === "mobile_revoke")).toBe(false);
	unpair.click();
	await settle();
	button(document.querySelector("dialog"), "Unpair").click();
	await settle();
	expect(calls.filter((c) => c.cmd === "mobile_revoke").map((c) => c.args)).toEqual([{ node_id: "7c1de0" }]);
});

test("Rename sends the trimmed name, and an empty one is not sent", async () => {
	devices = [phone("a3f9c2")];
	const t = await render(MobileSettings);
	button(t, "Rename").click();
	await settle();
	const dialog = document.querySelector("dialog");
	const input = dialog.querySelector("input");
	expect(input.value).toBe("");
	expect(button(dialog, "Save").disabled).toBe(true);
	input.value = "  Luan's iPhone ";
	input.dispatchEvent(new Event("input", { bubbles: true }));
	await settle();
	button(dialog, "Save").click();
	await settle();
	expect(calls.find((c) => c.cmd === "mobile_rename").args).toEqual({
		node_id: "a3f9c2",
		name: "Luan's iPhone",
	});
});

test("Activity reads the phone's log when it is opened", async () => {
	devices = [phone("a3f9c2")];
	const t = await render(MobileSettings);
	expect(calls.some((c) => c.cmd === "mobile_activity")).toBe(false);
	button(t, "Activity").click();
	await settle();
	expect(calls.find((c) => c.cmd === "mobile_activity").args).toEqual({ node_id: "a3f9c2", limit: 20 });
	expect(t.querySelector(".activity").textContent).toContain("Nothing yet");
});

test("a live code shows the QR, the grouped code, a countdown bar; the phone that scans it appears on its own", async () => {
	const t = await render(MobileSettings);
	button(t, "Show pairing code").click();
	await settle();
	const card = t.querySelector('[data-state="live"]');
	expect(card).not.toBeNull();
	expect(card.querySelector('[aria-label="Pairing QR code"] svg')).not.toBeNull();
	expect(card.querySelector(".code").textContent).toBe("012 345");
	expect(card.querySelector('[role="timer"]').textContent).toBe("Expires in 2:00");
	expect(card.querySelector('[role="progressbar"]')).not.toBeNull();
	expect(card.textContent).toContain("Waiting for your phone");
	// The phone scans: hostd now lists it. The pane is polling while the code is live (every second).
	devices = [phone("9a8b7c", { connected: true })];
	await nextDevicesRead();
	await settle();
	expect(t.querySelector('[data-state="live"]')).toBeNull();
	expect(t.querySelector('[data-state="idle"]').textContent).toContain("Phone 9A8B is paired");
	expect(t.querySelector(".row .label").textContent).toBe("Phone 9A8B");
});

test("a code that could not be made says why and offers another try", async () => {
	pairError = "mobile gateway: network is unreachable";
	const t = await render(MobileSettings);
	button(t, "Show pairing code").click();
	await settle();
	const card = t.querySelector('[data-state="idle"]');
	expect(card.textContent).toContain("Could not make a code");
	expect(card.textContent).toContain("network is unreachable");
	expect(button(card, "Show pairing code")).not.toBeUndefined();
});

test("an expired code says so and one click makes a new one", async () => {
	const now = Date.now();
	const started = [];
	const t = await render(PairCard, {
		pairing: { p: pairStartFixture, at: now - 121_000 },
		now,
		busy: false,
		error: null,
		paired: null,
		onstart: () => started.push(1),
	});
	expect(t.querySelector('[data-state="expired"]')).not.toBeNull();
	expect(t.textContent).toContain("Code expired");
	expect(t.querySelector('[aria-label="Pairing QR code"]')).toBeNull();
	button(t, "New code").click();
	expect(started).toEqual([1]);
});

test("the troubleshooting names the relay, the awake computer and the VPN docs", async () => {
	const t = await render(MobileSettings);
	button(t, "Pairing doesn't work?").click();
	await settle();
	const text = t.querySelector(".help").textContent;
	expect(text).toContain("does not need to be on this Wi-Fi");
	expect(text).toContain("Keep this computer awake");
	expect(t.querySelector('.help a[href$="#pairing-fails-with-a-vpn-or-warp-on"]')).not.toBeNull();
});
