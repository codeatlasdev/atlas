// @ts-nocheck bun test runtime types are not part of the app tsconfig
import { describe, expect, test } from "bun:test";
import {
	CHECK_EVERY_MS,
	CHANNELS,
	DEFAULT_CHANNEL,
	EMPTY_PREFS,
	EMPTY_PROGRESS,
	badgesFor,
	blurbOf,
	cardState,
	channelOfVersion,
	compareVersions,
	confirmCopy,
	laneStates,
	offerLine,
	parseChannelState,
	parseChannelViews,
	prereleaseBadge,
	dateOf,
	formatBytes,
	isSkipped,
	justUpdated,
	parseNotes,
	parsePrefs,
	percent,
	phaseFromLog,
	reduceProgress,
	selectNotes,
	shouldNotify,
	shouldRecheck,
	showPill,
	splitItem,
	stepsFrom,
	stripVersionHeading,
	versionAnchor,
} from "./updates.ts";
import { searchSettings } from "./settings/index.ts";
import fixture from "./__fixtures__/channel-views.json";

const rel = (version) => ({ version, url: "", published_at: "", notes: "" });
const prefs = (p = {}) => ({ ...EMPTY_PREFS, ...p });

describe("compareVersions", () => {
	test("numeric, not lexical; pre-release before release", () => {
		expect(compareVersions("1.10.0", "1.9.9")).toBe(1);
		expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
		expect(compareVersions("1.0.0-rc.1", "1.0.0")).toBe(-1);
		expect(compareVersions("v1.0.1", "1.0.0")).toBe(1);
	});

	test("pre-release identifiers follow SemVer precedence", () => {
		expect(compareVersions("1.4.0-alpha.10", "1.4.0-alpha.9")).toBe(1);
		expect(compareVersions("1.4.0-alpha.9", "1.4.0-alpha.10")).toBe(-1);
		expect(compareVersions("1.4.0-alpha.3", "1.4.0-beta.1")).toBe(-1);
		expect(compareVersions("1.4.0-beta.2", "1.4.0-rc.1")).toBe(-1);
		expect(compareVersions("1.4.0-rc.1", "1.4.0")).toBe(-1);
		expect(compareVersions("1.4.0-alpha", "1.4.0-alpha.1")).toBe(-1);
		expect(compareVersions("1.4.0-1", "1.4.0-alpha")).toBe(-1);
		expect(compareVersions("1.5.0-alpha.1", "1.4.0")).toBe(1);
	});

	test("build metadata is ignored", () => {
		expect(compareVersions("1.3.0+b2", "1.3.0+b1")).toBe(0);
		expect(compareVersions("1.3.0-beta.1+x", "1.3.0-beta.1")).toBe(0);
	});
});

describe("channelOfVersion", () => {
	test.each([
		["1.3.0", "stable"],
		["v1.3.0", "stable"],
		["1.4.0-rc.1", "beta"],
		["1.4.0-beta.2", "beta"],
		["1.4.0-alpha.3", "alpha"],
		["1.4.0-alpha", "alpha"],
		["1.4.0-nightly.1", null],
		["garbage", null],
		["", null],
	])("%s → %s", (v, want) => expect(channelOfVersion(v)).toBe(want));
});

describe("prereleaseBadge", () => {
	test("Alpha / Beta / RC for prereleases, nothing for a release or unknown label", () => {
		expect(prereleaseBadge("1.4.0-alpha.3")).toBe("Alpha");
		expect(prereleaseBadge("1.4.0-beta.2")).toBe("Beta");
		expect(prereleaseBadge("1.4.0-rc.1")).toBe("RC");
		expect(prereleaseBadge("1.4.0")).toBeNull();
		expect(prereleaseBadge("1.4.0-nightly.1")).toBeNull();
		expect(prereleaseBadge("junk")).toBeNull();
	});
});

// The fixture is written by the real Rust serializer (`channel_views_fixture_is_the_real_serialization`).
describe("contract with the Rust serializer", () => {
	test("channel views parse with their snake_case keys", () => {
		const views = parseChannelViews(fixture.views);
		expect(views.map((v) => v.channel)).toEqual(["stable", "beta", "alpha"]);
		const beta = views[1];
		expect(beta.risk_label).toBe(fixture.views[1].risk_label);
		expect(beta.switch_to).toBe(fixture.views[1].switch_to);
		expect(beta.offered).toBe("1.4.0-beta.2");
		expect(beta.tag).toBe("v1.4.0-beta.2");
		expect(beta.action).toBe("update");
		expect(views[0].selected).toBe(true);
		expect(views[0].alpha_era_note).toBe(fixture.views[0].alpha_era_note);
	});

	test("an offline check (no tag, no offer) still parses", () => {
		const views = parseChannelViews(fixture.offline);
		expect(views).toHaveLength(3);
		for (const v of views) {
			expect(v.offered).toBeNull();
			expect(v.tag).toBeNull();
			expect(v.action).toBe("none");
		}
		expect(views.find((v) => v.selected)?.channel).toBe("beta");
	});

	test("a downgrade offer is carried", () => {
		expect(parseChannelViews(fixture.ahead)[0].action).toBe("downgrade");
	});

	test("channel state parses", () => {
		expect(parseChannelState(fixture.state)).toEqual({
			channel: "beta",
			stage: fixture.state.stage,
			stage_note: fixture.state.stage_note,
			env_override: false,
		});
		expect(parseChannelState(fixture.state_override).env_override).toBe(true);
	});

	test("camelCase drift is rejected, not silently accepted", () => {
		const camel = fixture.views.map(({ risk_label, ...r }) => ({ ...r, riskLabel: risk_label }));
		expect(() => parseChannelViews(camel)).toThrow();
		expect(() => parseChannelViews("nope")).toThrow();
		expect(() => parseChannelViews([{ channel: "nightly" }])).toThrow();
		expect(() => parseChannelState({ channel: "beta" })).toThrow();
	});

	test("the Release payload keeps its keys", () => {
		const r = fixture.release;
		expect(r.update).toMatchObject({ channel: "beta", action: "update", downgrade: false, app_behind: true });
		expect(r.downgrade).toMatchObject({
			action: "downgrade",
			downgrade: true,
			app_ahead: true,
			app_behind: false,
		});
		expect(r.none).toMatchObject({ action: "none", cli_behind: false, cli_ahead: false });
	});
});

describe("offerLine", () => {
	const view = (action, offered) => ({ action, offered });
	test("says what the channel offers", () => {
		expect(offerLine(view("update", "1.4.0-beta.2"))).toBe("Offers 1.4.0-beta.2");
		expect(offerLine(view("none", "1.3.0"))).toBe("Nothing newer than what you run");
		expect(offerLine(view("downgrade", "1.3.0"))).toBe("Offers 1.3.0, older than yours");
		expect(offerLine(view("none", null))).toBe("Offer unknown");
	});
});

describe("DEFAULT_CHANNEL", () => {
	// Incident guard: the pre-load fallback of the channel cards was a hard-coded "stable"; it must follow the Rust default.
	test("is the variant marked #[default] on atlas_core::channel::Channel", async () => {
		const rs = await Bun.file(
			new URL("../../../../crates/atlas-core/src/channel.rs", import.meta.url),
		).text();
		const rustDefault = rs.match(/#\[default\]\s*(\w+)/)?.[1]?.toLowerCase();
		expect(DEFAULT_CHANNEL).toBe(rustDefault);
		expect(CHANNELS).toContain(DEFAULT_CHANNEL);
	});
	test("a profile with no channel file is shown following the default, before and after the real state loads", () => {
		const followed = (c) =>
			cardState(parseChannelViews(fixture.views), "1.4.0", c)
				.filter((x) => x.following)
				.map((x) => x.view.channel);
		expect(followed(DEFAULT_CHANNEL)).toEqual([DEFAULT_CHANNEL]);
		expect(parseChannelState(fixture.state_default).channel).toBe(DEFAULT_CHANNEL);
	});
});

describe("cardState", () => {
	const views = () => parseChannelViews(fixture.views);
	test("marks the followed channel and the one matching the running build", () => {
		const cards = cardState(views(), "1.3.0", "stable");
		expect(cards.map((c) => [c.view.channel, c.following, c.installed])).toEqual([
			["stable", true, true],
			["beta", false, false],
			["alpha", false, false],
		]);
		const onAlpha = cardState(views(), "1.4.0-alpha.3", "alpha");
		expect(onAlpha.map((c) => c.installed)).toEqual([false, false, true]);
		expect(onAlpha[2].following).toBe(true);
	});

	test("a build with an unknown label is installed on no card", () => {
		expect(cardState(views(), "1.4.0-nightly.1", "stable").some((c) => c.installed)).toBe(false);
		expect(cardState(views(), undefined, "stable").some((c) => c.installed)).toBe(false);
	});

	test("risk tone and offer line per card", () => {
		const cards = cardState(views(), "1.3.0", "stable");
		expect(cards.map((c) => c.tone)).toEqual(["success", "warn", "error"]);
		expect(cards[1].offer).toBe("Offers 1.4.0-beta.2");
		expect(cards[0].offer).toBe("Nothing newer than what you run");
	});
});

describe("confirmCopy", () => {
	const [stable, beta] = parseChannelViews(fixture.views);
	test("switching to a channel that offers an update never downgrades", () => {
		const c = confirmCopy(beta);
		expect(c.text).toBe(beta.switch_to);
		expect(c.downgrade).toBeNull();
	});
	test("nothing offered: just the switch text", () => {
		expect(confirmCopy(stable).downgrade).toBeNull();
	});
	test("a lower channel offers an explicit, separate downgrade with a warning", () => {
		const [low] = parseChannelViews(fixture.ahead);
		const c = confirmCopy(low);
		expect(c.text).toBe(low.switch_to);
		expect(c.downgrade).toEqual({
			label: "Install 1.3.0 now (downgrade)",
			version: "1.3.0",
			warning: expect.stringContaining("may not be readable"),
		});
	});
});

describe("laneStates", () => {
	test("the followed lane is lit and everything upstream feeds it", () => {
		expect(laneStates("stable", null)).toEqual({ alpha: "feeds", beta: "feeds", stable: "followed" });
		expect(laneStates("beta", null)).toEqual({ alpha: "feeds", beta: "followed", stable: "idle" });
		expect(laneStates("alpha", null)).toEqual({ alpha: "followed", beta: "idle", stable: "idle" });
	});
	test("an unconfirmed pick takes the focus instead of the followed channel", () => {
		expect(laneStates("alpha", "stable")).toEqual({ alpha: "feeds", beta: "feeds", stable: "pending" });
	});
});

describe("selectNotes", () => {
	const all = [rel("1.0.0"), rel("1.0.2"), rel("0.9.0"), rel("1.0.1"), rel("1.0.2"), rel("1.0.3")];
	test("every version in (installed, latest], newest first, deduplicated", () => {
		expect(selectNotes(all, "1.0.0", "1.0.2").map((r) => r.version)).toEqual(["1.0.2", "1.0.1"]);
	});
	test("installed == latest yields just that release", () => {
		expect(selectNotes(all, "1.0.1", "1.0.1").map((r) => r.version)).toEqual(["1.0.1"]);
	});
	test("nothing newer yields nothing", () => {
		expect(selectNotes(all, "1.0.3", "1.0.3").map((r) => r.version)).toEqual(["1.0.3"]);
		expect(selectNotes([rel("1.0.0")], "1.0.0", "1.0.2")).toEqual([]);
	});
});

describe("skip", () => {
	test("skips exactly that version; a newer one comes back", () => {
		const p = prefs({ skipped: "1.0.2" });
		expect(isSkipped(p, "1.0.2")).toBe(true);
		expect(isSkipped(p, "1.0.3")).toBe(false);
		expect(isSkipped(p, null)).toBe(false);
	});
	test("pill hides when skipped, linked, or nothing is available", () => {
		expect(showPill(true, false, prefs(), "1.0.2")).toBe(true);
		expect(showPill(true, false, prefs({ skipped: "1.0.2" }), "1.0.2")).toBe(false);
		expect(showPill(true, true, prefs(), "1.0.2")).toBe(false);
		expect(showPill(false, false, prefs(), "1.0.2")).toBe(false);
	});
	test("persisted shape survives a round trip and garbage", () => {
		const p = prefs({ skipped: "1.0.2", notified: "1.0.2", lastSeen: "1.0.1", auto: false });
		expect(parsePrefs(JSON.parse(JSON.stringify(p)))).toEqual(p);
		expect(parsePrefs(undefined)).toEqual(EMPTY_PREFS);
		expect(parsePrefs({ skipped: 5, auto: "no" })).toEqual({ ...EMPTY_PREFS, auto: true });
	});
});

describe("shouldNotify", () => {
	const on = { notificationsOn: true, linked: false };
	test("once per version", () => {
		expect(shouldNotify(prefs(), "1.0.2", on)).toBe(true);
		expect(shouldNotify(prefs({ notified: "1.0.2" }), "1.0.2", on)).toBe(false);
		expect(shouldNotify(prefs({ notified: "1.0.2" }), "1.0.3", on)).toBe(true);
	});
	test("not when notifications are off, dev build, skipped or no version", () => {
		expect(shouldNotify(prefs(), "1.0.2", { ...on, notificationsOn: false })).toBe(false);
		expect(shouldNotify(prefs(), "1.0.2", { ...on, linked: true })).toBe(false);
		expect(shouldNotify(prefs({ skipped: "1.0.2" }), "1.0.2", on)).toBe(false);
		expect(shouldNotify(prefs(), null, on)).toBe(false);
	});
});

describe("justUpdated", () => {
	test("only when the running version is newer than the last seen one", () => {
		expect(justUpdated(prefs({ lastSeen: "1.0.1" }), "1.0.2")).toBe(true);
		expect(justUpdated(prefs({ lastSeen: "1.0.2" }), "1.0.2")).toBe(false);
		expect(justUpdated(prefs({ lastSeen: "1.0.3" }), "1.0.2")).toBe(false);
		expect(justUpdated(prefs(), "1.0.2")).toBe(false);
	});
});

describe("shouldRecheck", () => {
	test("on focus at most every 6h, and never with auto-check off", () => {
		const now = 10 * CHECK_EVERY_MS;
		expect(shouldRecheck(prefs(), null, now)).toBe(true);
		expect(shouldRecheck(prefs(), now - 1000, now)).toBe(false);
		expect(shouldRecheck(prefs(), now - CHECK_EVERY_MS, now)).toBe(true);
		expect(shouldRecheck(prefs({ auto: false }), null, now)).toBe(false);
	});
});

describe("progress reducer", () => {
	const run = (...events) => events.reduce(reduceProgress, EMPTY_PROGRESS);
	const phase = (id) => ({ kind: "phase", id });
	const states = (s) => s.map((x) => x.state);

	test("phases only move forward", () => {
		expect(run(phase("verify"), phase("download")).at).toBe(1);
		expect(run(phase("nope")).at).toBe(-1);
	});
	test("bytes belong to the current phase and reset on the next", () => {
		const mid = run(phase("download"), { kind: "bytes", done: 50, total: 200 });
		expect(percent(mid)).toBe(25);
		const next = reduceProgress(mid, phase("verify"));
		expect([next.done, next.total, percent(next)]).toEqual([0, 0, null]);
	});
	test("percent clamps and is null while the size is unknown", () => {
		expect(percent({ ...EMPTY_PROGRESS, done: 300, total: 200 })).toBe(100);
		expect(percent({ ...EMPTY_PROGRESS, done: 5, total: 0 })).toBeNull();
	});
	test("the host outcome is remembered", () => {
		const p = run({ kind: "host", state: "needs-restart", live: 2 });
		expect(p.host).toEqual({ state: "needs-restart", live: 2 });
	});
	test("rollback is remembered", () => {
		expect(run({ kind: "rollback", state: "restored" }).rollback).toBe("restored");
		expect(run({ kind: "rollback", state: "failed" }).rollback).toBe("failed");
	});
	test("steps: download is active before any report", () => {
		expect(states(stepsFrom(EMPTY_PROGRESS, "running"))).toEqual([
			"active",
			"pending",
			"pending",
			"pending",
			"pending",
			"pending",
		]);
	});
	test("steps follow the phase; failed marks the active one", () => {
		const p = run(phase("download"), phase("verify"), phase("install-cli"));
		expect(states(stepsFrom(p, "running"))).toEqual([
			"done",
			"done",
			"active",
			"pending",
			"pending",
			"pending",
		]);
		expect(states(stepsFrom(p, "failed"))).toEqual([
			"done",
			"done",
			"failed",
			"pending",
			"pending",
			"pending",
		]);
	});
	test("done completes the install; relaunch waits for the restart", () => {
		expect(states(stepsFrom(EMPTY_PROGRESS, "done"))).toEqual([
			"done",
			"done",
			"done",
			"done",
			"done",
			"pending",
		]);
		expect(states(stepsFrom(EMPTY_PROGRESS, "done", true))[5]).toBe("active");
	});
	test("an older CLI's lines still map to phases", () => {
		expect(phaseFromLog("◆ Downloading v1.0.2...")).toEqual(phase("download"));
		expect(phaseFromLog("◆ Installing...")).toEqual(phase("install-cli"));
		expect(phaseFromLog("◆ Updating Atlas Desktop to v1.0.2...")).toEqual(phase("update-app"));
		expect(phaseFromLog("◆ Downloading Atlas-1.0.2-aarch64.app.tar.gz...")).toEqual(phase("update-app"));
		expect(phaseFromLog("hello")).toBeNull();
	});
});

describe("dateOf", () => {
	// Whatever the machine's timezone, a calendar date is that same day (not the UTC day before).
	test("a date-only string is its own local calendar day", () => {
		const expected = new Date(2026, 9, 12).toLocaleDateString("en-US", {
			year: "numeric",
			month: "short",
			day: "numeric",
		});
		expect(dateOf("2026-10-12", "en-US")).toBe(expected);
		expect(dateOf("2026-10-12", "en-US")).toBe("Oct 12, 2026");
	});
	test("impossible or empty dates give nothing", () => {
		expect(dateOf("2026-02-31", "en-US")).toBe("");
		expect(dateOf("", "en-US")).toBe("");
		expect(dateOf("soon", "en-US")).toBe("");
	});
	test("a full timestamp still parses", () => {
		expect(dateOf("2026-10-12T15:00:00Z", "en-US")).toMatch(/Oct 1[23], 2026/);
	});
});

test("formatBytes", () => {
	expect(formatBytes(0)).toBe("");
	expect(formatBytes(512)).toBe("1 KB");
	expect(formatBytes(860 * 1024)).toBe("860 KB");
	expect(formatBytes(12.4 * 1024 * 1024)).toBe("12.4 MB");
});

describe("parseNotes", () => {
	const md = [
		"## [1.2.1] — 2026-10-12",
		"",
		"Faster startup and a new theme system.",
		"",
		"Atlas is in **alpha**: expect bugs. Please [report](https://x.y).",
		"",
		"### Added",
		"",
		"- **Language servers, built in.** Completion for `Rust` and Go.",
		"- **Plain: colon.** Body.",
		"- No title here, just text.",
		"",
		"### Fixed",
		"",
		"- Window drag failed.",
		"  It works again.",
		"- **Faster startup.** First screen loads less:",
		"  - editor",
		"  - charts",
		"",
		"### Weird",
		"",
		"- x",
		"",
		"### Empty",
	].join("\n");
	const n = parseNotes(md);

	test("summary is the first paragraph; the alpha boilerplate is split out", () => {
		expect(n.summary).toBe("Faster startup and a new theme system.");
		expect(n.alpha).toContain("alpha");
	});
	test("groups keep order, map kinds and drop empty ones", () => {
		expect(n.groups.map((g) => [g.kind, g.heading, g.items.length])).toEqual([
			["added", "Added", 3],
			["fixed", "Fixed", 2],
			["other", "Weird", 1],
		]);
	});
	test("`**Title.** body` splits into title and body", () => {
		expect(n.groups[0].items[0]).toEqual({
			title: "Language servers, built in",
			body: "Completion for `Rust` and Go.",
		});
		expect(n.groups[0].items[1].title).toBe("Plain: colon");
	});
	test("a bullet without the convention keeps the whole text as body", () => {
		expect(n.groups[0].items[2]).toEqual({ title: "", body: "No title here, just text." });
	});
	test("continuation lines and nested bullets stay in the body", () => {
		expect(n.groups[1].items[0].body).toBe("Window drag failed.\nIt works again.");
		expect(n.groups[1].items[1].body).toBe("First screen loads less:\n\n- editor\n\n- charts");
	});
	test("notes without summary or groups", () => {
		expect(parseNotes("")).toEqual({ summary: "", alpha: "", groups: [] });
		expect(parseNotes("- **A.** b").groups).toEqual([]);
	});
	test("a bold lead with nothing after it is not a title", () => {
		expect(splitItem("**Only bold**")).toEqual({ title: "", body: "**Only bold**" });
		expect(splitItem("**T** — body")).toEqual({ title: "T", body: "body" });
	});
});

test("badgesFor", () => {
	expect(badgesFor("1.2.1", "1.1.0", "1.2.1", false)).toEqual(["latest"]);
	expect(badgesFor("1.1.0", "1.1.0", "1.2.1", false)).toEqual(["installed"]);
	expect(badgesFor("1.2.1", "1.2.1", "1.2.1", false)).toEqual(["latest", "installed"]);
	expect(badgesFor("1.2.1", "1.2.1", "1.2.1", true)).toEqual(["latest", "new"]);
	expect(badgesFor("1.2.0", "1.1.0", "1.2.1", false)).toEqual([]);
});

test("versionAnchor is a safe DOM id", () => {
	expect(versionAnchor("1.2.1")).toBe("v-1-2-1");
	expect(versionAnchor("1.2.1-rc.1")).toBe("v-1-2-1-rc-1");
});

test("stripVersionHeading drops the leading release heading only", () => {
	expect(stripVersionHeading("## [1.0.1] — 2026-10-11\n\n### Changed\n\n- x")).toBe("### Changed\n\n- x");
	expect(stripVersionHeading("### Changed\n- x")).toBe("### Changed\n- x");
});

test("blurbOf prefers the summary, then the first change, and caps its length", () => {
	expect(blurbOf(parseNotes("Faster startup.\n\n### Added\n- **A.** b"))).toBe("Faster startup.");
	expect(blurbOf(parseNotes("### Fixed\n- **No more flash.** The window opens hidden."))).toBe(
		"No more flash",
	);
	expect(blurbOf(parseNotes("### Fixed\n- Plain `code` text."))).toBe("Plain code text.");
	expect(blurbOf(parseNotes("### Fixed\n- " + "x".repeat(200))).length).toBe(90);
	expect(blurbOf(parseNotes(""))).toBe("");
});

describe("settings search", () => {
	test("the update channel is findable and lives in General", () => {
		for (const q of ["update channel", "beta", "alpha", "stable", "prerelease"]) {
			expect(searchSettings(q).some((e) => e.id === "update-channel" && e.section === "general")).toBe(true);
		}
	});
});
