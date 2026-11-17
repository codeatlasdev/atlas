// Test helpers for scripts/i18n/*.test.ts: a tiny, valid repository built in a temp dir (so every rule can be
// broken one file at a time), and runners that go through the real checkers, never around them.
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { runChecks } from "./check.ts";
import { fix } from "./fix.ts";
import { type Finding, loadCtx } from "./types.ts";
import { canonicalJson } from "./util.ts";

const dirs: string[] = [];

export function cleanup() {
	// Best effort: on Windows git's read-only object files can refuse deletion, and a leftover temp dir is no failure.
	for (const d of dirs.splice(0)) {
		try {
			rmSync(d, { recursive: true, force: true });
		} catch {}
	}
}

export const json = (obj: Record<string, unknown>) => canonicalJson(obj, "\t");

export const PLURAL = (one: string, other: string) => [
	{
		declarations: ["input count", "local countPlural = count: plural"],
		selectors: ["countPlural"],
		match: { "countPlural=one": one, "countPlural=*": other },
	},
];

export const LOCALES = json({ default: "en", locales: { en: { tier: "source" }, "pt-BR": { tier: "required", owners: ["@mvilacad"] } } });

export const GLOSSARY = `- term: Atlas
  dnt: true
- term: branch
  dnt: true
- term: settings
  pt-BR: configurações
`;

const EN = {
	app_files_count: PLURAL("{count} file", "{count} files"),
	app_open_branch: "Open branch",
	app_open_docs: "Read the {#link}docs{/link}",
	app_save_button: "Save",
	app_welcome: "Welcome to Atlas, {name}",
};

const PT = {
	app_files_count: PLURAL("{count} arquivo", "{count} arquivos"),
	app_open_branch: "Abrir branch",
	app_open_docs: "Leia a {#link}documentação{/link}",
	app_save_button: "Salvar",
	app_welcome: "Bem-vindo ao Atlas, {name}",
};

const DESCRIPTIONS = {
	app_files_count: "Count of changed files in the commit bar; plural",
	app_open_branch: "Menu item that opens the branch picker",
	app_open_docs: "Footer link; the {#link} tag wraps the word docs",
	app_save_button: "Primary toolbar button that saves the open file",
	app_welcome: "Greeting at the top of the welcome screen",
};

const APP = `<script>
  import * as m from "$lib/paraglide/messages";
  let { name } = $props();
</script>
<button>{m.app_save_button()}</button>
<p>{m.app_files_count({ count: 2 })} {m.app_welcome({ name })} {m.app_open_branch()} {m.app_open_docs()}</p>
`;

/** The files of a valid repo: one Paraglide app (`apps/desktop`), the i18n config, a Rust crate with one Fluent
 * catalog, and one translated doc. Keys of `over` replace a file; a `null` value removes it. */
export function goodFiles(over: Record<string, string | null> = {}): Record<string, string> {
	const files: Record<string, string> = {
		"i18n/locales.json": LOCALES,
		"i18n/glossary.yaml": GLOSSARY,
		"apps/desktop/project.inlang/settings.json": json({ $schema: "https://inlang.com/schema/project-settings", baseLocale: "en", locales: ["en", "pt-BR"] }),
		"apps/desktop/messages/en.json": json(EN),
		"apps/desktop/messages/pt-BR.json": json(PT),
		"apps/desktop/messages/descriptions.json": json(DESCRIPTIONS),
		"apps/desktop/src/App.svelte": APP,
		"crates/atlas-i18n/locales/en/cli.ftl": `# Header of the \`atlas config lang\` output
cli-lang-current = Language: { $lang }

# Summary line of \`atlas files\`; plural
cli-files-count =
    { $count ->
        [one] { $count } file
       *[other] { $count } files
    }
`,
		"crates/atlas-i18n/locales/pt-BR/cli.ftl": `cli-lang-current = Idioma: { $lang }

cli-files-count =
    { $count ->
        [0] Nenhum arquivo
        [one] { $count } arquivo
       *[other] { $count } arquivos
    }
`,
		"docs/getting-started.md": "# Getting started\n\nInstall Atlas.\n\n```sh\nbun install\n```\n",
	};
	for (const [k, v] of Object.entries(over)) if (v === null) delete files[k];
	else files[k] = v;
	return files;
}

export function writeFiles(root: string, files: Record<string, string>) {
	for (const [p, text] of Object.entries(files)) {
		mkdirSync(dirname(join(root, p)), { recursive: true });
		writeFileSync(join(root, p), text);
	}
}

/** The git blob sha `docs/<source>` must be recorded as. */
export async function blobSha(root: string, path: string): Promise<string> {
	return (await Bun.$`git hash-object ${join(root, path)}`.quiet().text()).trim();
}

/** A temp repo with `files`, locks and baseline stamped by the real `i18n:fix`, plus a valid translated doc. */
export async function makeRepo(over: Record<string, string | null> = {}): Promise<string> {
	const root = mkdtempSync(join(tmpdir(), "i18n-test-"));
	dirs.push(root);
	const files = goodFiles(over);
	writeFiles(root, files);
	if (!("docs/pt-BR/getting-started.md" in over)) {
		const sha = await blobSha(root, "docs/getting-started.md");
		const doc = "# Primeiros passos\n\nInstale o Atlas.\n\n```sh\nbun install\n```\n";
		writeFiles(root, { "docs/pt-BR/getting-started.md": `---\ntitle: "Primeiros passos"\nsource: getting-started.md\nsource_sha: "${sha}"\n---\n\n${doc}` });
	}
	fix(loadCtx(root), { baseline: true });
	return root;
}

/** Edit one file of a repo built by `makeRepo`, run all checkers, return the findings. */
export async function check(root: string, surface?: string): Promise<Finding[]> {
	return runChecks(loadCtx(root, surface));
}

export const read = (root: string, p: string) => readFileSync(join(root, p), "utf8");
export const put = (root: string, p: string, text: string) => writeFiles(root, { [p]: text });

/** `rule` of the findings with that severity, for compact assertions. */
export const rulesOf = (fs: Finding[], severity?: Finding["severity"]) => fs.filter((f) => !severity || f.severity === severity).map((f) => f.rule);
export const errors = (fs: Finding[]) => fs.filter((f) => f.severity === "error");

export function editJson(root: string, p: string, fn: (o: Record<string, unknown>) => void) {
	const o = JSON.parse(read(root, p));
	fn(o);
	put(root, p, json(o));
}

/** Add a key (English, pt-BR, description) to the app, reference it from App.svelte, and re-stamp the locks. */
export function addKey(root: string, key: string, en: unknown, pt: unknown, description: string) {
	editJson(root, "apps/desktop/messages/en.json", (o) => void (o[key] = en));
	editJson(root, "apps/desktop/messages/pt-BR.json", (o) => void (o[key] = pt));
	editJson(root, "apps/desktop/messages/descriptions.json", (o) => void (o[key] = description));
	put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<i>{m.${key}()}</i>\n`);
	fix(loadCtx(root));
}

export const finding = (fs: Finding[], rule: string, key?: string) => fs.find((f) => f.rule === rule && (key === undefined || f.key === key));
