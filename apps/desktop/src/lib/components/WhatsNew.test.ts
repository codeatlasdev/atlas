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
let invokeImpl = async (_cmd, _args) => ({});
mock.module("@tauri-apps/api/core", () => ({ invoke: (cmd, args) => invokeImpl(cmd, args) }));
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

// Incident (first-user pass, shot 26): the sheet for 1.5.0-alpha.25 said "No release notes for this version / The
// changelog has no entry for it yet" although CHANGELOG.md has `## [1.5.0-alpha.25]`. A sheet must end in one of three
// states: the notes, "Couldn't load" with a reason and Retry, or a "no entry" that names the version.
const CHANGELOG = (await Bun.file(`${import.meta.dir}/../../../../../CHANGELOG.md`).text()).replace(
	/\r\n/g,
	"\n",
);
/** The body of `## [version]` in the real CHANGELOG.md, the way the Rust parser hands it to the sheet. */
const sectionOf = (version) => {
	const head = CHANGELOG.indexOf(`## [${version}]`);
	if (head < 0) throw new Error(`CHANGELOG.md has no [${version}]`);
	const body = CHANGELOG.slice(CHANGELOG.indexOf("\n", head) + 1);
	return body.slice(0, body.search(/^## \[/m)).trim();
};

const running = (version) => ({
	app: { version, dev: false },
	cli: { version },
});

async function openFresh(handler, info = running("1.5.0-alpha.25")) {
	u.info = info;
	u.release = null;
	u.notesError = null;
	u.notesFor = null;
	invokeImpl = handler;
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(WhatsNew, { target, props: {} }));
	done = () => (unmount(app), target.remove());
	u.showWhatsNew();
	for (let i = 0; i < 50 && u.notesLoading; i++) await tick();
	await tick();
	flushSync();
	return document.body;
}

afterEach(() => {
	invokeImpl = async () => ({});
	u.info = null;
	u.release = null;
	u.notesError = null;
	u.notesFor = null;
});

for (const version of ["1.5.0-alpha.25", "1.5.0-alpha.26"]) {
	test(`the real CHANGELOG.md entry of ${version} renders as notes, not as "no release notes"`, async () => {
		const notes = sectionOf(version);
		expect(notes.length).toBeGreaterThan(200);
		const body = await openFresh(
			async (cmd) => (cmd === "releases_between" ? [release(version, notes)] : {}),
			running(version),
		);
		// The element, not the text: the notes themselves may quote "No release notes" (1.5.0-alpha.26 does).
		expect(body.querySelector('[data-state="empty"]')).toBeNull();
		expect(body.querySelector(".num")?.textContent).toBe(version);
		expect(body.querySelectorAll(".group").length).toBeGreaterThan(0);
		expect(body.querySelectorAll("li").length).toBeGreaterThan(2);
	});
}

test("a failed read says it could not load, why, and Retry loads the notes", async () => {
	let calls = 0;
	const body = await openFresh(async (cmd) => {
		if (cmd !== "releases_between") return {};
		if (calls++ === 0) throw "GitHub request failed: curl: (6) Could not resolve host";
		return [release("1.5.0-alpha.25")];
	});
	expect(body.textContent).toContain("Couldn't load the release notes");
	expect(body.textContent).toContain("Could not resolve host");
	expect(body.querySelector('[data-state="empty"]')).toBeNull();
	const retry = [...body.querySelectorAll("button")].find((b) => b.textContent.trim() === "Retry");
	expect(retry).toBeDefined();
	retry.click();
	for (let i = 0; i < 50 && u.notesLoading; i++) await tick();
	await tick();
	flushSync();
	expect(body.textContent).not.toContain("Couldn't load");
	expect(body.querySelector(".num")?.textContent).toBe("1.5.0-alpha.25");
});

test("a failure with no message is still a failure with a reason, never the empty-changelog state", async () => {
	for (const empty of ["", "  ", new Error("")]) {
		const body = await openFresh(async (cmd) => {
			if (cmd === "releases_between") throw empty;
			return {};
		});
		expect(body.textContent, JSON.stringify(empty)).toContain("Couldn't load the release notes");
		expect(body.querySelector('[data-state="empty"]')).toBeNull();
		expect(body.querySelector(".state p")?.textContent.trim().length).toBeGreaterThan(0);
		done();
		done = undefined;
		u.whatsNewOpen = false;
	}
});

test("a version the changelog really lacks says so, naming it", async () => {
	const body = await openFresh(
		async (cmd) => (cmd === "releases_between" ? [] : {}),
		running("1.5.0-alpha.99"),
	);
	expect(body.querySelector('[data-state="empty"]')?.textContent).toContain(
		"No release notes for 1.5.0-alpha.99",
	);
	expect(body.textContent).toContain("The changelog has no entry for it yet");
	expect(body.textContent).not.toContain("Couldn't load");
});

test("opening the sheet before the running version is known reads it instead of showing an empty sheet", async () => {
	const asked = [];
	const body = await openFresh(async (cmd, args) => {
		asked.push(cmd);
		if (cmd === "system_info") return running("1.5.0-alpha.25");
		if (cmd === "releases_between") return [release(args.to)];
		return {};
	}, null);
	expect(asked).toEqual(["system_info", "releases_between"]);
	expect(body.querySelector(".num")?.textContent).toBe("1.5.0-alpha.25");
	expect(body.querySelector('[data-state="empty"]')).toBeNull();
});

test("opening the sheet when even the running version cannot be read says it could not load", async () => {
	const body = await openFresh(async (cmd) => {
		if (cmd === "system_info") throw "cannot run atlas";
		return {};
	}, null);
	expect(body.textContent).toContain("Couldn't load the release notes");
	expect(body.textContent).toContain("cannot run atlas");
});
