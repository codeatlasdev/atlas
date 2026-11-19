// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { afterEach, expect, mock, test } from "bun:test";
import { inLocale } from "../../../test/locale.ts";

// A message called once and stored keeps the language of that moment, so a Settings > Display language switch left it
// behind (batch S10 found the class; `i18n:check` rule `frozen` now stops new ones). These three are real labels of
// three kinds: a command registered when a component mounts, a label in a module-level list, a settings row. Each is read
// AFTER the language changed at run time (the locale store), the way the palette, the crew catalog and Settings read them.
// Dynamic imports below: they must load after the mocks are installed.
mock.module("@tauri-apps/api/core", () => ({ invoke: async () => null }));
mock.module("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
mock.module("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({}) }));
const { flushSync, mount, unmount } = await import("svelte");
const { setAppLocale } = await import("#lib/i18n/locale.svelte.ts");
const { m } = await import("#lib/i18n/m.ts");
const { registeredCommands } = await import("#lib/keymap.svelte.ts");
const { default: ActionOutputDrawer } = await import("#lib/panel/ActionOutputDrawer.svelte");
const { CATALOG } = await import("#lib/crew/integrations.ts");
const { REGISTRY } = await import("#lib/settings/registry.ts");

let done: (() => void) | undefined;
afterEach(() => {
	done?.();
	done = undefined;
	setAppLocale("en");
	document.body.innerHTML = "";
});

const commandLabel = (id: string) => registeredCommands().find((c) => c.id === id)?.label;

test("a command registered while the app runs in English is listed in Portuguese after the switch", () => {
	const target = document.createElement("div");
	document.body.append(target);
	const app = flushSync(() => mount(ActionOutputDrawer, { target, props: { project: { id: "p1" } } }));
	done = () => unmount(app);
	flushSync();

	const id = "atlas::ShowActionOutput";
	expect(commandLabel(id)).toBe(m.panel_action_output_drawer_show_action_output());
	const english = commandLabel(id);

	setAppLocale("pt-BR");
	const portuguese = commandLabel(id);
	expect(portuguese).toBe(m.panel_action_output_drawer_show_action_output());
	expect(portuguese).not.toBe(english);

	setAppLocale("en");
	expect(commandLabel(id)).toBe(english);
});

test("a label in a module-level list follows the language when it is read", async () => {
	const github = CATALOG.find((i) => i.id === "github");
	const english = github.description;
	expect(english).toBe(m.crew_workspace_integrations_github_description());
	const portuguese = await inLocale("pt-BR", () => github.description);
	expect(portuguese).toBe(await inLocale("pt-BR", () => m.crew_workspace_integrations_github_description()));
	expect(portuguese).not.toBe(english);
});

test("a settings row and its group title follow the language when Settings reads them", async () => {
	const general = REGISTRY.find((c) => c.id === "general");
	const group = general.pages
		.flatMap((p) => p.groups)
		.find((g) => g.items.some((i) => i.id === "display-language"));
	const row = group.items.find((i) => i.id === "display-language");
	const english = { title: group.title, label: row.label, description: row.description };
	expect(english.label).toBe(m.settings_display_language_label());

	const portuguese = await inLocale("pt-BR", () => ({
		title: group.title,
		label: row.label,
		description: row.description,
	}));
	expect(portuguese.title).not.toBe(english.title);
	expect(portuguese.label).not.toBe(english.label);
	expect(portuguese.description).not.toBe(english.description);
	expect(portuguese.label).toBe(await inLocale("pt-BR", () => m.settings_display_language_label()));
});
