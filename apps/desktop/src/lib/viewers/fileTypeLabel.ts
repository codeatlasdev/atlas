/**
 * The name of a file type for people: a proper name as written, or the translated description of a described type
 * (`Compose file`, `Lockfile`). Its own module so the ~30 messages load with the screens that show them (the editor,
 * the tree viewer), not with `fileType.ts`, which the startup bundle imports.
 */
import { m } from "#lib/i18n/m.ts";
import type { FileType } from "#lib/viewers/fileType.ts";

const DESCRIBED = {
	compose: () => m.viewers_filetypes_compose(),
	license: () => m.viewers_filetypes_license(),
	notice: () => m.viewers_filetypes_notice(),
	jsonc: () => m.viewers_filetypes_jsonc(),
	notebook: () => m.viewers_filetypes_notebook(),
	env: () => m.viewers_filetypes_env(),
	lock: () => m.viewers_filetypes_lock(),
	config: () => m.viewers_filetypes_config(),
	properties: () => m.viewers_filetypes_properties(),
	prisma: () => m.viewers_filetypes_prisma(),
	terraformVars: () => m.viewers_filetypes_terraform_vars(),
	shell: () => m.viewers_filetypes_shell(),
	bash: () => m.viewers_filetypes_bash(),
	zsh: () => m.viewers_filetypes_zsh(),
	fish: () => m.viewers_filetypes_fish(),
	kotlinScript: () => m.viewers_filetypes_kotlin_script(),
	pythonStub: () => m.viewers_filetypes_python_stub(),
	cHeader: () => m.viewers_filetypes_c_header(),
	cppHeader: () => m.viewers_filetypes_cpp_header(),
	elixirScript: () => m.viewers_filetypes_elixir_script(),
	log: () => m.viewers_filetypes_log(),
	diff: () => m.viewers_filetypes_diff(),
	patch: () => m.viewers_filetypes_patch(),
	http: () => m.viewers_filetypes_http(),
	plist: () => m.viewers_filetypes_plist(),
	xmlSchema: () => m.viewers_filetypes_xml_schema(),
	text: () => m.viewers_filetypes_text(),
	githubWorkflow: () => m.viewers_filetypes_github_workflow(),
	githubAction: () => m.viewers_filetypes_github_action(),
};

export type DescId = keyof typeof DESCRIBED;

export const fileTypeLabel = (ft: FileType): string => (ft.desc ? DESCRIBED[ft.desc]() : ft.label);
