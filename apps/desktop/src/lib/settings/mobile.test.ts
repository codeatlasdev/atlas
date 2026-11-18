// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import devicesFixture from "../__fixtures__/mobile-devices.json";
import pairStartFixture from "../__fixtures__/mobile-pair-start.json";
import activityFixture from "../__fixtures__/mobile-activity.json";
import { dateTime, relativeTime } from "#lib/i18n/fmt.ts";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";
import {
	clock,
	deviceTitle,
	groupCode,
	isGenericName,
	MOBILE_APP,
	pairUrl,
	SCOPE_IDS,
	secondsLeft,
	sortDevices,
	toggleScope,
	VPN_DOCS,
} from "./mobile.ts";
import { access, describeActivity, deviceStatus, scopeGroups, scopeText } from "./mobileText.ts";

// The REAL `mobile_pair_start` reply (written by atlas-host `mobile::tests`), never a hand-made mock.
const p = pairStartFixture;
// The URL below is what the phone's `pair_url_test.dart` parses; UPDATE_FIXTURES=1 rewrites it.
const urlFixture = join(import.meta.dir, "../../../../mobile/test/fixtures/pair_url.txt");

describe("pairUrl", () => {
	test("carries every field of the real reply, a hostname with spaces and quotes, repeated addrs", () => {
		const url = pairUrl(p);
		const u = new URL(url);
		expect(u.protocol).toBe("atlas-mobile:");
		expect(u.searchParams.get("node")).toBe(p.node_id);
		expect(u.searchParams.get("code")).toBe(p.code);
		expect(u.searchParams.get("name")).toBe(p.hostname);
		expect(u.searchParams.getAll("addr")).toEqual(p.addrs);
		expect(u.searchParams.has("relay")).toBe(false);
		if (process.env.UPDATE_FIXTURES) writeFileSync(urlFixture, `${url}\n`);
		expect(readFileSync(urlFixture, "utf8").trim()).toBe(url);
	});
});

test("the real devices reply has the fields Settings reads", () => {
	const [d] = devicesFixture;
	expect(typeof d.node_id).toBe("string");
	expect(typeof d.name).toBe("string");
	expect(typeof d.paired_at).toBe("number");
	expect(typeof d.revoked).toBe("boolean");
	expect(d.scopes.every((s) => SCOPE_IDS.includes(s))).toBe(true);
});

test("secondsLeft counts down and stops at zero", () => {
	expect(secondsLeft(p, 1000, 1000)).toBe(120);
	expect(secondsLeft(p, 1000, 1000 + 119_500)).toBe(1);
	expect(secondsLeft(p, 1000, 1000 + 500_000)).toBe(0);
	// A cached clock reading from before the code was issued must not show more than the TTL.
	expect(secondsLeft(p, 1000, 400)).toBe(120);
});

test("toggleScope keeps canonical order and no duplicates", () => {
	expect(toggleScope(["term_write", "read"], "act", true)).toEqual(["read", "act", "term_write"]);
	expect(toggleScope(["read", "act"], "act", true)).toEqual(["read", "act"]);
	expect(toggleScope(["read", "act"], "read", false)).toEqual(["act"]);
});

// The helpers read the REAL replies (written by atlas-host `mobile::tests`), not a hand-made mock.
const [real] = devicesFixture;
const NOW = (real.last_seen + 125) * 1000;

describe("what the list says about a phone", () => {
	test("the real devices reply carries what the list needs: connection and last seen", () => {
		expect(typeof real.connected).toBe("boolean");
		expect(typeof real.last_seen).toBe("number");
	});

	test("the name every Atlas Mobile sends names nobody: the title is Phone + the start of its key", () => {
		expect(real.name).toBe("Atlas Mobile");
		expect(isGenericName(real.name)).toBe(true);
		expect(deviceTitle({ name: "Atlas Mobile", node_id: "a3f9c2" })).toBe("Phone A3F9");
		expect(deviceTitle({ name: "ATLAS MOBILE ", node_id: "7c1de0" })).toBe("Phone 7C1D");
		expect(deviceTitle({ name: "  Luan's iPhone ", node_id: "a3f9c2" })).toBe("Luan's iPhone");
	});

	test("the status line: connected now, last seen relative, never connected, always the paired date", () => {
		const paired = dateTime(real.paired_at * 1000, { dateStyle: "medium" });
		expect(deviceStatus({ ...real, connected: true }, NOW)).toBe(`Connected now · Paired ${paired}`);
		expect(deviceStatus(real, NOW)).toBe(`Last seen 2 min ago · Paired ${paired}`);
		const never = { ...real, last_seen: undefined };
		expect(deviceStatus(never, NOW)).toBe(`Not connected yet · Paired ${paired}`);
	});

	test("in another language the line is the catalog's, with that language's date and relative time", async () => {
		await inLocale("pt-BR", () => {
			const paired = dateTime(real.paired_at * 1000, { dateStyle: "medium" });
			const seen = relativeTime(real.last_seen * 1000, NOW);
			expect(deviceStatus(real, NOW)).toBe(m.mobile_device_status_seen({ seen, paired }));
			expect(seen).not.toBe("2 min ago");
		});
	});

	test("the connected phone first, then the most recently seen", () => {
		const d = (id: string, o: object) => ({ ...real, node_id: id, connected: false, ...o });
		const order = sortDevices([
			d("old", { last_seen: 100, paired_at: 1 }),
			d("live", { connected: true, last_seen: 50, paired_at: 2 }),
			d("new", { last_seen: 900, paired_at: 3 }),
			d("never", { last_seen: undefined, paired_at: 4 }),
		]).map((x) => x.node_id);
		expect(order).toEqual(["live", "new", "old", "never"]);
	});
});

describe("permissions in words", () => {
	test("every scope is in exactly one group, with a risk line", () => {
		const groups = scopeGroups();
		expect(groups.flatMap((g) => g.scopes.map((s) => s.id)).sort()).toEqual([...SCOPE_IDS].sort());
		for (const g of groups) for (const s of g.scopes) expect(s.risk.length).toBeGreaterThan(10);
	});

	test("full, view only, none and custom", () => {
		expect(access(SCOPE_IDS)).toMatchObject({ label: "Full access", kind: "full" });
		expect(access(real.scopes)).toMatchObject({ label: "View only", kind: "view" });
		expect(access(["read"]).detail).toContain("Cannot change anything");
		expect(access([])).toMatchObject({ label: "No access", kind: "none" });
		expect(access(["read", "act", "approve"])).toMatchObject({ label: "Custom: 3 of 5", kind: "custom" });
		// Typing alone is not "view only", and neither is terminals without the list they live in.
		expect(access(["term_read"]).kind).toBe("custom");
		expect(access(["read", "term_write"]).kind).toBe("custom");
	});

	test("a custom summary lists the held permissions in lower case, in the display language", async () => {
		expect(access(["read", "act"]).detail).toBe(
			"Can: see projects, logs and agents; restart services, run actions, send prompts.",
		);
		await inLocale("pt-BR", () => {
			const [read, act] = ["read", "act"].map((id) => scopeText(id).label.toLocaleLowerCase("pt-BR"));
			expect(access(["read", "act"]).detail).toBe(
				m.mobile_access_custom_detail({ scopes: `${read}; ${act}` }),
			);
		});
	});
});

describe("the activity, from the real audit lines", () => {
	test("readable sentences; typed text is only a size", () => {
		expect(activityFixture.map(describeActivity)).toEqual([
			"Allowed an agent's tool call",
			"Typed in a terminal (3 bytes)",
			"Restarted web",
		]);
		expect(describeActivity({ ts: 1, cmd: "agent_send", args: { text: "<42 chars>" } })).toBe(
			"Sent a prompt to an agent (42 characters)",
		);
	});

	test("a command the list does not know is shown by its name, never blank", () => {
		expect(describeActivity({ ts: 1, cmd: "crewmate_memory_set", args: {} })).toBe("crewmate_memory_set");
		expect(describeActivity({ ts: 1, cmd: "", args: null })).toBe("Unknown action");
		expect(describeActivity({ ts: 1, cmd: "project_service", args: { op: "restart" } })).toBe(
			"Restarted the stack",
		);
		expect(describeActivity({ ts: 1, cmd: "pty_write", args: { data: "<4 bytes base64>" } })).toBe(
			"Typed in a terminal (3 bytes)",
		);
		expect(describeActivity({ ts: 1, cmd: "agent_send", args: { text: "<1 chars>" } })).toBe(
			"Sent a prompt to an agent (1 character)",
		);
	});
});

test("the code is shown in groups and the countdown as m:ss", () => {
	expect(groupCode("012345")).toBe("012 345");
	expect(clock(102)).toBe("1:42");
	expect(clock(120)).toBe("2:00");
	expect(clock(-3)).toBe("0:00");
});

test("the store links live in one constant; none is invented, the docs page and its VPN anchor are there", () => {
	expect(MOBILE_APP.docs).toBe("https://atlas.codeatlas.com.br/docs/mobile/");
	expect(MOBILE_APP.appStore === null || MOBILE_APP.appStore.startsWith("https://apps.apple.com/")).toBe(
		true,
	);
	expect(MOBILE_APP.playStore === null || MOBILE_APP.playStore.startsWith("https://play.google.com/")).toBe(
		true,
	);
	expect(VPN_DOCS).toBe(`${MOBILE_APP.docs}#pairing-fails-with-a-vpn-or-warp-on`);
});
