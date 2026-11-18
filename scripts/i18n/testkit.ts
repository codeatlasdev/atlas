// Test helpers for scripts/i18n/*.test.ts: a tiny, valid repository built in a temp dir (so every rule can be broken one
// file at a time), and runners that go through the real checkers, never around them.
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { runChecks } from "./check.ts";
import { type Baseline, renderBaseline } from "./baseline.ts";
import { fix } from "./fix.ts";
import { BASELINE_FILE, type Finding, loadCtx } from "./types.ts";
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

/** A plural with an exact zero as a second selector: CLDR puts 0 in pt-BR's `one`, so "0 arquivo" needs its own arm. */
export const PLURAL_ZERO = (zero: string, one: string, other: string) => [
	{
		declarations: ["input count", "local countExact = count", "local countPlural = count: plural"],
		selectors: ["countExact", "countPlural"],
		match: { "countExact=0, countPlural=*": zero, "countExact=*, countPlural=one": one, "countExact=*, countPlural=*": other },
	},
];

/** A select on `os`: mac, windows, and the default. */
export const OS = (mac: string, windows: string, other: string) => [
	{ declarations: ["input os"], selectors: ["os"], match: { "os=mac": mac, "os=windows": windows, "os=*": other } },
];

export const LOCALES = json({ default: "en", locales: { en: { tier: "source" }, "pt-BR": { tier: "required", owners: ["@mvilacad"] } } });

export const GLOSSARY = `- term: Atlas
  dnt: true
- term: branch
  dnt: true
- term: settings
  pt-BR: configurações
`;

/** Paths of an area's files in the fixture. */
export const area = (name: string, file: string) => `i18n/messages/${name}/${file}.json`;
export const EN = area("app", "en");
export const PT = area("app", "pt-BR");
export const META = area("app", "meta");
export const CLI_EN = area("cli", "en");
export const CLI_PT = area("cli", "pt-BR");
export const CLI_META = area("cli", "meta");

export const SURFACES = `{
	"budget": { "keys": 150, "bytes": 24576 },
	"surfaces": {
		"desktop": { "paraglide": "apps/desktop/.i18n" },
		"rust": { "rust": "crates" }
	}
}
`;

/** The area.json that says an area is consumed by `surface`. */
export const areaFile = (surface: string, generated?: string) => canonicalJson({ surface, ...(generated ? { generated } : {}) });

const APP_EN = {
	app_files_count: PLURAL("{count} file", "{count} files"),
	app_open_branch: "Open branch",
	app_open_docs: "Read the {#link}docs{/link}",
	app_save_button: "Save",
	app_welcome: "Welcome to Atlas, {name}",
};

const APP_PT = {
	app_files_count: PLURAL_ZERO("Nenhum arquivo", "{count} arquivo", "{count} arquivos"),
	app_open_branch: "Abrir branch",
	app_open_docs: "Leia a {#link}documentação{/link}",
	app_save_button: "Salvar",
	app_welcome: "Bem-vindo ao Atlas, {name}",
};

const APP_META = {
	app_files_count: "Count of changed files in the commit bar; plural",
	app_open_branch: "Menu item that opens the branch picker",
	app_open_docs: "Footer link; the {#link} tag wraps the word docs",
	app_save_button: "Primary toolbar button that saves the open file",
	app_welcome: "Greeting at the top of the welcome screen",
};

const CLI_EN_V = {
	cli_files_count: PLURAL("{count} file", "{count} files"),
	cli_lang_current: "Language: {lang}",
};

const CLI_PT_V = {
	cli_files_count: PLURAL_ZERO("Nenhum arquivo", "{count} arquivo", "{count} arquivos"),
	cli_lang_current: "Idioma: {lang}",
};

const CLI_META_V = {
	cli_files_count: "Summary line of `atlas files`; plural",
	cli_lang_current: "Header of the `atlas config lang` output",
};

const APP = `<script>
  import * as m from "$lib/paraglide/messages";
  let { name } = $props();
</script>
<button>{m.app_save_button()}</button>
<p>{m.app_files_count({ count: 2 })} {m.app_welcome({ name })} {m.app_open_branch()} {m.app_open_docs()}</p>
`;

const RUST = `pub fn lang() -> String {
    t!("cli_lang_current", lang = "en")
}
pub fn files(n: u32) -> String {
    t!("cli_files_count", count = n)
}
`;

const COMMON_EN = { common_action_cancel: "Cancel", common_action_delete: "Delete" };
const COMMON_PT = { common_action_cancel: "Cancelar", common_action_delete: "Excluir" };
const COMMON_META = { common_action_cancel: "Button that dismisses a dialog without applying it", common_action_delete: "Button that deletes the selected item" };
const COMMON_OS_EN = { common_os_trash: OS("Trash", "Recycle Bin", "Trash") };
const COMMON_OS_PT = { common_os_trash: OS("Lixeira", "Lixeira", "Lixeira") };
const COMMON_OS_META = { common_os_trash: "Where deleted files go: Moved to {@common_os_trash}" };

const LANG_RS = 'pub const SUPPORTED: &[&str] = &["en", "pt-BR"];\n';
const PLIST = `<plist version="1.0">
<dict>
\t<key>CFBundleLocalizations</key>
\t<array>
\t\t<string>en</string>
\t\t<string>pt-BR</string>
\t</array>
</dict>
</plist>
`;

/** The files of a valid repo: three areas of messages (app for desktop, cli for rust, the common ones), the surfaces,
 * the Paraglide project, a Svelte file and a Rust file that use every key, the language lists, one translated doc.
 * Keys of `over` replace a file; a `null` value removes it. */
export function goodFiles(over: Record<string, string | null> = {}): Record<string, string> {
	const files: Record<string, string> = {
		"i18n/locales.json": LOCALES,
		"i18n/glossary.yaml": GLOSSARY,
		"i18n/surfaces.json": SURFACES,
		[area("app", "en")]: json(APP_EN),
		[area("app", "pt-BR")]: json(APP_PT),
		[area("app", "meta")]: json(APP_META),
		[area("app", "area")]: areaFile("desktop"),
		[area("cli", "en")]: json(CLI_EN_V),
		[area("cli", "pt-BR")]: json(CLI_PT_V),
		[area("cli", "meta")]: json(CLI_META_V),
		[area("cli", "area")]: areaFile("rust"),
		[area("common", "en")]: json(COMMON_EN),
		[area("common", "pt-BR")]: json(COMMON_PT),
		[area("common", "meta")]: json(COMMON_META),
		[area("common_os", "en")]: json(COMMON_OS_EN),
		[area("common_os", "pt-BR")]: json(COMMON_OS_PT),
		[area("common_os", "meta")]: json(COMMON_OS_META),
		"apps/desktop/project.inlang/settings.json": json({ baseLocale: "en", locales: ["en", "pt-BR"] }),
		"apps/desktop/src/App.svelte": APP,
		"crates/atlas-i18n/src/lib.rs": RUST,
		"crates/atlas-core/src/lang.rs": LANG_RS,
		"crates/atlas-desktop/Info.plist": PLIST,
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
	writeFiles(root, goodFiles(over));
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
/** `git` in a temp repo with a throwaway identity and no signing; `must` throws on failure and returns the trimmed output. */
export function git(root: string, ...args: string[]) {
	const r = Bun.spawnSync(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", ...args], { cwd: root, stdout: "pipe", stderr: "pipe" });
	return { ok: r.exitCode === 0, out: r.stdout.toString().trim(), err: r.stderr.toString().trim() };
}
export const must = (root: string, ...args: string[]) => {
	const r = git(root, ...args);
	if (!r.ok) throw new Error(`git ${args.join(" ")}: ${r.err}\n${r.out}`);
	return r.out;
};

/** The baseline file of a repo, and the text of a baseline in its line format. */
export const BASELINE = BASELINE_FILE;
export const baselineText = (b: Baseline) => renderBaseline(b);

export const put = (root: string, p: string, text: string) => writeFiles(root, { [p]: text });

/** `rule` of the findings with that severity, for compact assertions. */
export const rulesOf = (fs: Finding[], severity?: Finding["severity"]) => fs.filter((f) => !severity || f.severity === severity).map((f) => f.rule);
export const errors = (fs: Finding[]) => fs.filter((f) => f.severity === "error");

export function editJson(root: string, p: string, fn: (o: Record<string, unknown>) => void) {
	const o = JSON.parse(read(root, p));
	fn(o);
	put(root, p, json(o));
}

/** Add a key (English, pt-BR, description) to the app area, reference it from App.svelte, and re-stamp the locks. */
export function addKey(root: string, key: string, en: unknown, pt: unknown, description: string) {
	editJson(root, EN, (o) => void (o[key] = en));
	editJson(root, PT, (o) => void (o[key] = pt));
	editJson(root, META, (o) => void (o[key] = description));
	put(root, "apps/desktop/src/App.svelte", `${read(root, "apps/desktop/src/App.svelte")}<i>{m.${key}()}</i>\n`);
	fix(loadCtx(root));
}

export const finding = (fs: Finding[], rule: string, key?: string) => fs.find((f) => f.rule === rule && (key === undefined || f.key === key));
