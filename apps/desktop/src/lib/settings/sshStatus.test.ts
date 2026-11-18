// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { describe, expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { inLocale } from "../../../test/locale.ts";
import { LABEL_MAX, sshStatusView } from "./sshStatus.ts";

// The strings `set_status` in crates/atlas-desktop/src/remote.rs writes: an id, or `warning:`/`error:` plus ssh's text.
const WARNING = "warning: its atlas hostd lacks mobile, crew; update atlas on the host and restart its hostd";
const ERROR = "error: example-host: tunnel up but hostd unreachable: connection refused";

describe("SSH host status chip", () => {
	test("each known id gets its own message and color", () => {
		expect(sshStatusView("connected")).toEqual({ tone: "success", label: m.settings_agents_ssh_status_connected() });
		expect(sshStatusView("connecting")).toEqual({ tone: "warning", label: m.settings_agents_ssh_status_connecting() });
		expect(sshStatusView("removed")).toEqual({ tone: "neutral", label: m.settings_agents_ssh_status_removed() });
		expect(sshStatusView(undefined)).toEqual({ tone: "neutral", label: m.settings_agents_ssh_status_idle() });
	});

	test("a warning is connected-but-outdated and keeps the shell's own text for the row below", () => {
		expect(sshStatusView(WARNING)).toEqual({
			tone: "warning",
			label: m.settings_agents_ssh_status_outdated(),
			detail: WARNING,
		});
	});

	test("an error is one word on the chip and the whole reason below, never cut", () => {
		const long = `${ERROR} ${"x".repeat(200)}`;
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
		const english = sshStatusView("connected").label;
		const portuguese = await inLocale("pt-BR", () => sshStatusView("connected").label);
		expect(portuguese).toBe(await inLocale("pt-BR", () => m.settings_agents_ssh_status_connected()));
		expect(portuguese).not.toBe(english);
	});
});
