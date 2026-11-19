// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { named } from "#lib/keymap.svelte.ts";
import { STATE_LABEL } from "./rightPanel.ts";
import { inLocale } from "../../../test/locale.ts";

// Commands are registered once at startup; a label frozen then would keep the first language after a switch.
test("a command named by a message reads the language when the palette lists it", async () => {
	const [cmd] = named([
		{ id: "pane::ActivateNextItem", label: () => m.shell_commands_next_tab(), run: () => {} },
	]);
	const en = cmd.label;
	expect(en).toBe(m.shell_commands_next_tab());
	const pt = await inLocale("pt-BR", () => cmd.label);
	expect(pt).toBe(await inLocale("pt-BR", () => m.shell_commands_next_tab()));
	expect(pt).not.toBe(en);
	expect(cmd.label).toBe(en);
});

test("the agent state words follow the language on every read", async () => {
	const en = STATE_LABEL.working;
	const pt = await inLocale("pt-BR", () => STATE_LABEL.working);
	expect(en).toBe(m.shell_chrome_agent_state_working());
	expect(pt).not.toBe(en);
	expect(STATE_LABEL.blocked).toBe(STATE_LABEL.waiting);
});
