// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";

// Incident (user screenshot, macOS WKWebView): the What's new sheet showed
//  - the channel badge ("Alpha") as a tall pill dropped below the date: the chip carried the class `alpha`, which is
//    also the sheet's footer-note block (`.alpha { margin: 26px 0 0; padding-top: 12px; border-top }`), so the badge
//    inherited its margin, padding and rule. The footer's "release · following Alpha" chip had the same collision.
//  - the release intro as raw markdown (`**The Windows release.**`, `` `Atlas-Setup.exe` ``): the summary was printed as
//    text while the bullets went through `renderNote`.
//  - item titles with literal backticks: the title was printed as text as well.
// Real kit Drawer, real store, real renderer: what is asserted is the markup a reader gets.
// Dynamic imports on purpose: the Svelte loader registered by test/setup.ts must exist before these modules load.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => ({}) }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (await import("../../../test/uiKit.ts")).useRealKit("Drawer", "Button", "Spinner");
const { flushSync, mount, tick, unmount } = await import("svelte");
const { updates: u } = await import("#lib/stores/updates.svelte.ts");
const { default: WhatsNew } = await import("./WhatsNew.svelte");

const WINDOWS = [
	"**The Windows release.** Atlas Desktop on Windows 10/11 now installs with one click (`Atlas-Setup.exe`), so",
	'Windows SmartScreen may say "Windows protected your PC": click **More info**, then **Run anyway**.',
	"",
	"Atlas is in alpha: expect rough edges.",
	"",
	"### Changed",
	"",
	"- **Opening a folder without an `atlas.yaml` now shows what Atlas found.** The panel names the stack.",
].join("\n");

function release(version, notes = WINDOWS) {
	return { version, url: `https://x.y/${version}`, published_at: "2026-11-12", notes };
}

let done;
afterEach(() => {
	done?.();
	done = undefined;
	u.whatsNewOpen = false;
	u.notes = [];
	u.channelState = null;
});

async function open(notes, followed = "alpha") {
	u.info = { app: { version: "1.5.0-alpha.24", dev: false }, cli: { version: "1.5.0-alpha.24" } };
	u.release = { version: notes[0].version, url: "", cli_behind: true, app_behind: true };
	u.channelState = { channel: followed };
	u.notes = notes;
	u.whatsNewOpen = true;
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(WhatsNew, { target, props: {} }));
	done = () => (unmount(app), target.remove());
	await tick();
	flushSync();
	return document.body;
}

test("the intro renders its markdown: bold and code spans, never literal ** or backticks", async () => {
	const body = await open([release("1.5.0-alpha.25")]);
	const intro = body.querySelector(".summary");
	expect(intro).not.toBeNull();
	expect(intro.textContent).not.toContain("**");
	expect(intro.textContent).not.toContain("`");
	expect([...intro.querySelectorAll("strong")].map((s) => s.textContent)).toEqual([
		"The Windows release.",
		"More info",
		"Run anyway",
	]);
	expect(intro.querySelector("code")?.textContent).toBe("Atlas-Setup.exe");
});

test("an item title renders its code spans: no literal backticks in the heading", async () => {
	const body = await open([release("1.5.0-alpha.25")]);
	const title = body.querySelector("li .title");
	expect(title.textContent).toBe("Opening a folder without an atlas.yaml now shows what Atlas found");
	expect(title.querySelector("code")?.textContent).toBe("atlas.yaml");
});

test("a release without an intro renders no empty summary paragraph", async () => {
	const body = await open([release("1.5.0-alpha.25", "### Fixed\n\n- **A.** b")]);
	expect(body.querySelector(".summary")).toBeNull();
});

// The class collision: a badge named after its channel must not be styled by an unrelated block with the same class.
test("no badge carries a bare channel name as a class (alpha/beta/rc/stable are note and layout classes too)", async () => {
	const body = await open([release("1.5.0-alpha.25")], "alpha");
	const chips = [...body.querySelectorAll(".chip")];
	expect(chips.map((c) => c.textContent.trim())).toEqual(expect.arrayContaining(["Alpha", "Latest"]));
	for (const c of chips) {
		for (const k of ["alpha", "beta", "rc", "stable"])
			expect(c.classList.contains(k), c.outerHTML).toBe(false);
	}
	// The footer note keeps its own class and is a different element from every badge.
	const note = body.querySelector("p.alpha");
	expect(note).not.toBeNull();
	expect(chips.includes(note)).toBe(false);
});

test("every prerelease gets its badge in the header, and the footer names the channel it ships on", async () => {
	// The header badge is the prerelease label; the footer is the CHANNEL (a release candidate ships on beta).
	for (const [version, label, channel] of [
		["1.5.0-alpha.25", "Alpha", "alpha"],
		["1.5.0-beta.2", "Beta", "beta"],
		["1.5.0-rc.1", "RC", "beta"],
	]) {
		const body = await open([release(version)], "beta");
		const pre = body.querySelector(".meta .chip[data-channel]");
		expect(pre?.textContent.trim(), version).toBe(label);
		expect(pre.dataset.channel, version).toBe(label.toLowerCase());
		const foot = body.querySelector(".chan .chip[data-channel]");
		expect(foot?.dataset.channel, version).toBe(channel);
		done();
		done = undefined;
		u.whatsNewOpen = false;
	}
});

test("a stable release has no prerelease badge in the header but still names its channel in the footer", async () => {
	const body = await open([release("1.5.0")], "stable");
	expect(body.querySelector(".meta .chip[data-channel]")).toBeNull();
	expect(body.querySelector(".chan .chip[data-channel]")?.dataset.channel).toBe("stable");
});
