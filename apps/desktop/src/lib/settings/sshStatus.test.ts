// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";
// The REAL strings the shell sends, one per kind of status: written by `ssh_status_fixture_is_what_rust_sends` in
// crates/atlas-desktop/src/remote.rs from `SshStatus::wire` (`UPDATE_FIXTURES=1` rewrites it).
import fixture from "../__fixtures__/ssh-status.json";
import { LABEL_MAX, sshStatusView } from "./sshStatus.ts";

/** What the chip must show for each kind the shell can send. A kind missing here is a status the page cannot map. */
const CHIPS = {
	connecting: { tone: "warning", label: () => m.settings_agents_ssh_status_connecting() },
	connected: { tone: "success", label: () => m.settings_agents_ssh_status_connected() },
	removed: { tone: "neutral", label: () => m.settings_agents_ssh_status_removed() },
	warning: { tone: "warning", label: () => m.settings_agents_ssh_status_outdated(), detail: true },
	error: { tone: "error", label: () => m.settings_agents_ssh_status_error(), detail: true },
};

describe("SSH host status chip", () => {
	test("the page maps every kind of status the shell sends, and the shell sends every kind the page maps", () => {
		expect(Object.keys(fixture).sort()).toEqual(Object.keys(CHIPS).sort());
	});

	test.each(Object.entries(fixture))("the shell's %s status gets its own message and color", (kind, wire) => {
		const chip = CHIPS[kind];
		expect(chip).toBeDefined();
		const view = sshStatusView(wire);
		expect(view.tone).toBe(chip.tone);
		expect(view.label).toBe(chip.label());
		// A warning or an error keeps the shell's own text for the row below; an id has none.
		expect(view.detail).toBe(chip.detail ? wire : undefined);
	});

	test("a host the shell has not reported on yet is idle", () => {
		expect(sshStatusView(undefined)).toEqual({ tone: "neutral", label: m.settings_agents_ssh_status_idle() });
	});

	test("an error is one word on the chip and the whole reason below, never cut", () => {
		const long = `${fixture.error} ${"x".repeat(200)}`;
		const v = sshStatusView(long);
		expect(v.tone).toBe("error");
		expect(v.label).toBe(m.settings_agents_ssh_status_error());
		expect(v.detail).toBe(long);
	});

	test("a status the app does not know shows raw, cut to fit the chip, with no color of its own", () => {
		const raw = `handshaking ${"y".repeat(100)}`;
		const v = sshStatusView(raw);
		expect(v.tone).toBe("neutral");
		expect(v.label).toBe(raw.slice(0, LABEL_MAX));
		expect(v.label.length).toBe(LABEL_MAX);
		expect(v.detail).toBeUndefined();
	});

	test("an id is matched whole: a longer word that starts like one is unknown, not connected", () => {
		expect(sshStatusView("connectedness").label).toBe("connectedness");
		expect(sshStatusView("connectedness").tone).toBe("neutral");
	});

	test("the label follows the display language when it changes", async () => {
		const english = sshStatusView(fixture.connected).label;
		const portuguese = await inLocale("pt-BR", () => sshStatusView(fixture.connected).label);
		expect(portuguese).toBe(await inLocale("pt-BR", () => m.settings_agents_ssh_status_connected()));
		expect(portuguese).not.toBe(english);
	});
});
