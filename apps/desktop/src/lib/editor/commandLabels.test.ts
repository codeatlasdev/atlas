// @ts-nocheck bun test runtime types are not part of the app tsconfig
import "../../../test/setup.ts";
import { expect, test } from "bun:test";
import { m } from "#lib/i18n/m.ts";
import { lspCommands } from "#lib/lsp/commands.ts";
import { inLocale } from "../../../test/locale.ts";
import { editorShellCommands } from "./shellCommands.ts";

// Commands are registered once at startup and the palette reads `label` from those same objects each time it opens:
// a label computed at registration would stay in the first language for the whole session.
test("editor and language server commands read their label in the app's current language", async () => {
	const list = [...editorShellCommands(), ...lspCommands()];
	const label = (id: string) => list.find((c) => c.id === id).label;
	const en = label("editor::FoldAllCommand");
	expect(en).toBe(m.workbench_editor_cmd_fold_all());
	await inLocale("pt-BR", () => {
		expect(label("editor::FoldAllCommand")).toBe(m.workbench_editor_cmd_fold_all());
		expect(label("editor::FoldAllCommand")).not.toBe(en);
		expect(label("editor::GoToDefinition")).toBe(m.workbench_lsp_cmd_go_to_definition());
		expect(label("editor::GoBack")).toBe(m.workbench_lsp_cmd_go_back());
		expect(label("editor::RevertFile")).toBe(m.workbench_editor_cmd_revert_file());
		expect(label("editor::ToggleSoftWrap")).toBe(m.workbench_editor_cmd_toggle_wrap());
	});
	expect(label("editor::FoldAllCommand")).toBe(en);
});
