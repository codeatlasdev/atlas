// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../../test/setup.ts";
import fixtureViews from "../../__fixtures__/channel-views.json";
import { afterEach, expect, mock, test } from "bun:test";

// Incident (first-user pass, shot 25): Settings > Updates showed "Atlas is up to date" and "Checking…" at once.
// Real section, real store: what is asserted is the card a reader sees while the store is in each state.
mock.module("@tauri-apps/api/core", () => ({
	// The real serialization of `update_channels` (written by the Rust tests), so the channel cards render for real.
	invoke: async (cmd) => (cmd === "update_channels" ? fixtureViews.views : null),
}));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
await (
	await import("../../../../test/uiKit.ts")
).useRealKit(
	"Button",
	"Spinner",
	"Switch",
	"Alert",
	"Kbd",
	"ToggleGroup/SegmentedControl",
	"Tooltip",
	"Skeleton",
);
const { flushSync, mount, unmount } = await import("svelte");
const { updates: u } = await import("#lib/stores/updates.svelte.ts");
const { default: UpdatesSection } = await import("./UpdatesSection.svelte");

const release = (version = "1.5.0-alpha.25") => ({
	version,
	url: "https://x.y/r",
	published_at: "",
	notes: "",
	channel: "alpha",
	action: "update",
	downgrade: false,
	cli_behind: false,
	app_behind: false,
	cli_ahead: false,
	app_ahead: false,
});

let done;
afterEach(() => {
	done?.();
	done = undefined;
	document.body.innerHTML = "";
	Object.assign(u, {
		info: null,
		release: null,
		error: null,
		checking: false,
		running: null,
		relaunchNeeded: false,
		checkedAt: null,
		channelState: null,
	});
});

function card(state) {
	u.info = {
		app: { version: "1.5.0-alpha.25", dev: false, bundle: "/x", linked_to: null },
		cli: { path: "/y", version: "1.5.0-alpha.25", linked_to: null, error: null },
	};
	Object.assign(u, state);
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(UpdatesSection, { target, props: {} }));
	done = () => unmount(app);
	flushSync();
	return {
		headline: target.querySelector(".hero h2").textContent.trim(),
		hero: target.querySelector(".hero").textContent.replace(/\s+/g, " "),
		updateButton: [...target.querySelectorAll(".hact button")].some((b) =>
			/Update CLI and app/.test(b.textContent),
		),
	};
}

test("while checking, the card says it is checking and gives no verdict, even with an earlier answer", () => {
	const c = card({ release: release(), checkedAt: Date.now(), checking: true });
	expect(c.headline).toBe("Checking for updates…");
	expect(c.hero).not.toMatch(/up to date/i);
	expect(c.hero.match(/Checking/g)).toHaveLength(1);
});

test("a finished check with nothing newer says up to date and nothing about checking", () => {
	const c = card({ release: release(), checkedAt: Date.now() });
	expect(c.headline).toBe("Atlas is up to date");
	expect(c.hero).not.toMatch(/Checking|Looking for/);
});

test("a finished check with a newer release offers the update; a check in flight takes the offer away until it answers", () => {
	const newer = { ...release("1.5.0-alpha.26"), app_behind: true, cli_behind: true };
	const shown = card({ release: newer, checkedAt: Date.now() });
	expect(shown.headline).toBe("Atlas 1.5.0-alpha.26 is available");
	expect(shown.updateButton).toBe(true);
	done();
	document.body.innerHTML = "";
	const again = card({ release: newer, checking: true });
	expect(again.headline).toBe("Checking for updates…");
	expect(again.updateButton).toBe(false);
});

test("a failed check is not 'up to date' just because an earlier one was", () => {
	const c = card({ release: release(), checkedAt: Date.now(), error: "offline" });
	expect(c.headline).toBe("Atlas versions");
	expect(c.hero).not.toMatch(/up to date/i);
});

// #313: the app runs a withdrawn version. The release is the real Rust serialization (fixture), so the field the
// section reads is the one hostd's `latest_release` really sends.
test("a withdrawn running version says why and that updating leaves it", () => {
	card({ release: fixtureViews.release.yanked, checkedAt: Date.now() });
	const text = document.body.textContent.replace(/\s+/g, " ");
	expect(text).toContain("This version was withdrawn");
	expect(text).toContain("Corrupts the workspace file on first launch.");
	expect(text).toContain("Use 1.4.0-beta.2 instead.");
	expect(text).toContain("Update to leave it.");
});

test("a version that was not withdrawn shows no withdrawal notice", () => {
	card({ release: fixtureViews.release.update, checkedAt: Date.now() });
	expect(document.body.textContent).not.toContain("withdrawn");
});

// The channel and the version in the "newer build than the channel offers" note come from two sources (the followed
// channel's display name and the real `latest_release` serialization), both interpolated into one message.
test("a build newer than the followed channel offers says which channel and which version", () => {
	card({
		release: fixtureViews.release.downgrade,
		checkedAt: Date.now(),
		channelState: { channel: "stable" },
	});
	const note = document.body.querySelector(".skipnote").textContent.replace(/\s+/g, " ");
	expect(note).toContain("newer build than the Stable channel offers (1.3.0)");
});

test("a finished check says when it finished", () => {
	const c = card({ release: release(), checkedAt: Date.now() });
	expect(c.hero).toContain("Checked at");
});
