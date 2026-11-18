/**
 * One pure mapping from a file path to everything the editor needs to know about it:
 * the syntax language (source mode), the optional rich ("eye") view, and its label.
 * Name first (Dockerfile, .env.local, Makefile), then the longest dotted suffix.
 */
import type { DescId } from "#lib/viewers/fileTypeLabel.ts";

/** Rich views, one component each under `lib/viewers/rich/`. Markdown/SVG predate this and keep their own viewers. */
export type RichView =
	| "env"
	| "package-json"
	| "dockerfile"
	| "compose"
	| "makefile"
	| "justfile"
	| "atlas"
	| "manifest"
	| "tsconfig"
	| "workflow"
	| "table"
	| "tree"
	| "ignore"
	| "license"
	| "codeowners"
	| "editorconfig"
	| "procfile"
	| "notebook";

export interface FileType {
	/** Stable id, also the key under which the rich/source choice is remembered. */
	id: string;
	/** Syntax-highlighting language name (see `loadLanguage`), `null` = plain text. */
	lang: string | null;
	/** Rich view toggled with the Eye button, when this file type has one. */
	view?: RichView;
	/** Proper name shown in tooltips (`Dockerfile`, `package.json`); empty for a described type: read it with `fileTypeLabel`. */
	label: string;
	/** Which described name this type has (`fileTypeLabel.ts`), when it has no proper name. */
	desc?: DescId;
	/** File name handed to the file-icon resolver (material-icon-theme). */
	icon: string;
}

/** A file type's name: a proper name as written (`Dockerfile`, `TypeScript`), or the id of a described one (`L.compose`), whose words are in `fileTypeLabel.ts` (kept out of the startup bundle). */
type Label = string | { desc: DescId };

const T = (id: string, label: Label, lang: string | null, view?: RichView, icon?: string): FileType => ({
	id,
	label: typeof label === "string" ? label : "",
	desc: typeof label === "string" ? undefined : label.desc,
	lang,
	view,
	icon: icon ?? id,
});

/** The file types whose name is a description, not a proper name. */
const L = {
	compose: { desc: "compose" as const },
	license: { desc: "license" as const },
	notice: { desc: "notice" as const },
	jsonc: { desc: "jsonc" as const },
	notebook: { desc: "notebook" as const },
	env: { desc: "env" as const },
	lock: { desc: "lock" as const },
	config: { desc: "config" as const },
	properties: { desc: "properties" as const },
	prisma: { desc: "prisma" as const },
	terraformVars: { desc: "terraformVars" as const },
	shell: { desc: "shell" as const },
	bash: { desc: "bash" as const },
	zsh: { desc: "zsh" as const },
	fish: { desc: "fish" as const },
	kotlinScript: { desc: "kotlinScript" as const },
	pythonStub: { desc: "pythonStub" as const },
	cHeader: { desc: "cHeader" as const },
	cppHeader: { desc: "cppHeader" as const },
	elixirScript: { desc: "elixirScript" as const },
	log: { desc: "log" as const },
	diff: { desc: "diff" as const },
	patch: { desc: "patch" as const },
	http: { desc: "http" as const },
	plist: { desc: "plist" as const },
	xmlSchema: { desc: "xmlSchema" as const },
	text: { desc: "text" as const },
	githubWorkflow: { desc: "githubWorkflow" as const },
	githubAction: { desc: "githubAction" as const },
};

/** Exact (lower-cased) base names. */
const BY_NAME: Record<string, FileType> = {
	"package.json": T("package-json", "package.json", "json", "package-json", "package.json"),
	"composer.json": T("composer", "composer.json", "json", "manifest", "composer.json"),
	"tsconfig.json": T("tsconfig", "tsconfig.json", "jsonc", "tsconfig", "tsconfig.json"),
	"jsconfig.json": T("tsconfig", "jsconfig.json", "jsonc", "tsconfig", "jsconfig.json"),
	"cargo.toml": T("cargo", "Cargo.toml", "toml", "manifest", "Cargo.toml"),
	"pyproject.toml": T("pyproject", "pyproject.toml", "toml", "manifest", "pyproject.toml"),
	"go.mod": T("go-mod", "go.mod", "go-mod", "manifest", "go.mod"),
	"go.sum": T("go-sum", "go.sum", null, undefined, "go.sum"),
	"cargo.lock": T("lock", "Cargo.lock", "toml", undefined, "Cargo.lock"),
	"atlas.yaml": T("atlas", "atlas.yaml", "yaml", "atlas", "atlas.yaml"),
	"atlas.yml": T("atlas", "atlas.yml", "yaml", "atlas", "atlas.yaml"),
	"atlas.local.yaml": T("atlas", "atlas.local.yaml", "yaml", "atlas", "atlas.yaml"),
	"atlas.local.yml": T("atlas", "atlas.local.yml", "yaml", "atlas", "atlas.yaml"),
	"docker-compose.yml": T("compose", L.compose, "yaml", "compose", "docker-compose.yml"),
	"docker-compose.yaml": T("compose", L.compose, "yaml", "compose", "docker-compose.yml"),
	"compose.yml": T("compose", L.compose, "yaml", "compose", "docker-compose.yml"),
	"compose.yaml": T("compose", L.compose, "yaml", "compose", "docker-compose.yml"),
	containerfile: T("dockerfile", "Containerfile", "dockerfile", "dockerfile", "Dockerfile"),
	makefile: T("makefile", "Makefile", "makefile", "makefile", "Makefile"),
	gnumakefile: T("makefile", "Makefile", "makefile", "makefile", "Makefile"),
	justfile: T("justfile", "Justfile", "justfile", "justfile", "justfile"),
	".justfile": T("justfile", "Justfile", "justfile", "justfile", "justfile"),
	procfile: T("procfile", "Procfile", "procfile", "procfile", "Procfile"),
	codeowners: T("codeowners", "CODEOWNERS", "gitignore", "codeowners", "CODEOWNERS"),
	".editorconfig": T("editorconfig", ".editorconfig", "properties", "editorconfig", ".editorconfig"),
	".gitignore": T("ignore", ".gitignore", "gitignore", "ignore", ".gitignore"),
	".dockerignore": T("ignore", ".dockerignore", "gitignore", "ignore", ".dockerignore"),
	".gitattributes": T("gitattributes", ".gitattributes", "gitignore", undefined, ".gitattributes"),
	".npmrc": T("rc", ".npmrc", "properties", undefined, ".npmrc"),
	".yarnrc": T("rc", ".yarnrc", "properties", undefined, ".yarnrc"),
	".nvmrc": T("rc", ".nvmrc", null, undefined, ".nvmrc"),
	".node-version": T("rc", ".node-version", null, undefined, ".nvmrc"),
	".tool-versions": T("rc", ".tool-versions", "properties", undefined, ".tool-versions"),
	".prettierrc": T("json", ".prettierrc", "json", "tree", ".prettierrc"),
	".eslintrc": T("json", ".eslintrc", "json", "tree", ".eslintrc"),
	".babelrc": T("json", ".babelrc", "json", "tree", ".babelrc"),
	"nginx.conf": T("nginx", "nginx.conf", "nginx", undefined, "nginx.conf"),
	"cmakelists.txt": T("cmake", "CMakeLists.txt", "cmake", undefined, "CMakeLists.txt"),
	jenkinsfile: T("groovy", "Jenkinsfile", "groovy", undefined, "Jenkinsfile"),
	gemfile: T("ruby", "Gemfile", "ruby", undefined, "Gemfile"),
	rakefile: T("ruby", "Rakefile", "ruby", undefined, "Rakefile"),
	vagrantfile: T("ruby", "Vagrantfile", "ruby", undefined, "Vagrantfile"),
	brewfile: T("ruby", "Brewfile", "ruby", undefined, "Brewfile"),
	podfile: T("ruby", "Podfile", "ruby", undefined, "Podfile"),
	dockerfile: T("dockerfile", "Dockerfile", "dockerfile", "dockerfile", "Dockerfile"),
	license: T("license", L.license, null, "license", "LICENSE"),
	licence: T("license", L.license, null, "license", "LICENSE"),
	copying: T("license", L.license, null, "license", "LICENSE"),
	unlicense: T("license", L.license, null, "license", "LICENSE"),
	notice: T("text", L.notice, null, undefined, "NOTICE"),
	".env": T("env", ".env", "env", "env", ".env"),
	".envrc": T("shell", ".envrc", "shell", undefined, ".envrc"),
	"bun.lock": T("lock", "bun.lock", "jsonc", undefined, "bun.lock"),
	"flake.lock": T("lock", "flake.lock", "json", "tree", "flake.lock"),
};

/** Longest-suffix extensions (`d.ts` before `ts`). */
const BY_EXT: Record<string, FileType> = {
	json: T("json", "JSON", "json", "tree", "file.json"),
	jsonc: T("jsonc", L.jsonc, "jsonc", "tree", "file.jsonc"),
	json5: T("json5", "JSON5", "jsonc", "tree", "file.json5"),
	yaml: T("yaml", "YAML", "yaml", "tree", "file.yaml"),
	yml: T("yaml", "YAML", "yaml", "tree", "file.yml"),
	toml: T("toml", "TOML", "toml", "tree", "file.toml"),
	csv: T("csv", "CSV", "csv", "table", "file.csv"),
	tsv: T("tsv", "TSV", "csv", "table", "file.tsv"),
	ipynb: T("notebook", L.notebook, "json", "notebook", "file.ipynb"),
	env: T("env", L.env, "env", "env", ".env"),
	dockerfile: T("dockerfile", "Dockerfile", "dockerfile", "dockerfile", "Dockerfile"),
	mk: T("makefile", "Makefile", "makefile", "makefile", "Makefile"),
	just: T("justfile", "Justfile", "justfile", "justfile", "justfile"),
	lock: T("lock", L.lock, "yaml", undefined, "file.lock"),
	ini: T("ini", "INI", "properties", undefined, "file.ini"),
	cfg: T("ini", L.config, "properties", undefined, "file.cfg"),
	conf: T("ini", L.config, "properties", undefined, "file.conf"),
	properties: T("properties", L.properties, "properties", undefined, "file.properties"),
	proto: T("proto", "Protocol Buffers", "protobuf", undefined, "file.proto"),
	graphql: T("graphql", "GraphQL", "graphql", undefined, "file.graphql"),
	gql: T("graphql", "GraphQL", "graphql", undefined, "file.gql"),
	prisma: T("prisma", L.prisma, "prisma", undefined, "file.prisma"),
	sql: T("sql", "SQL", "sql", undefined, "file.sql"),
	tf: T("terraform", "Terraform", "hcl", undefined, "file.tf"),
	tfvars: T("terraform", L.terraformVars, "hcl", undefined, "file.tfvars"),
	hcl: T("hcl", "HCL", "hcl", undefined, "file.hcl"),
	sh: T("shell", L.shell, "shell", undefined, "file.sh"),
	bash: T("shell", L.bash, "shell", undefined, "file.bash"),
	zsh: T("shell", L.zsh, "shell", undefined, "file.zsh"),
	fish: T("fish", L.fish, "shell", undefined, "file.fish"),
	ps1: T("powershell", "PowerShell", "powershell", undefined, "file.ps1"),
	psm1: T("powershell", "PowerShell", "powershell", undefined, "file.psm1"),
	lua: T("lua", "Lua", "lua", undefined, "file.lua"),
	rb: T("ruby", "Ruby", "ruby", undefined, "file.rb"),
	java: T("java", "Java", "java", undefined, "file.java"),
	kt: T("kotlin", "Kotlin", "kotlin", undefined, "file.kt"),
	kts: T("kotlin", L.kotlinScript, "kotlin", undefined, "file.kts"),
	swift: T("swift", "Swift", "swift", undefined, "file.swift"),
	c: T("c", "C", "c", undefined, "file.c"),
	h: T("c", L.cHeader, "c", undefined, "file.h"),
	cpp: T("cpp", "C++", "cpp", undefined, "file.cpp"),
	cc: T("cpp", "C++", "cpp", undefined, "file.cc"),
	cxx: T("cpp", "C++", "cpp", undefined, "file.cxx"),
	hpp: T("cpp", L.cppHeader, "cpp", undefined, "file.hpp"),
	cs: T("csharp", "C#", "csharp", undefined, "file.cs"),
	dart: T("dart", "Dart", "dart", undefined, "file.dart"),
	ex: T("elixir", "Elixir", "elixir", undefined, "file.ex"),
	exs: T("elixir", L.elixirScript, "elixir", undefined, "file.exs"),
	vue: T("vue", "Vue", "vue", undefined, "file.vue"),
	svelte: T("svelte", "Svelte", "svelte", undefined, "file.svelte"),
	astro: T("astro", "Astro", "astro", undefined, "file.astro"),
	log: T("log", L.log, "log", undefined, "file.log"),
	diff: T("diff", L.diff, "diff", undefined, "file.diff"),
	patch: T("diff", L.patch, "diff", undefined, "file.patch"),
	http: T("http", L.http, "http", undefined, "file.http"),
	rest: T("http", L.http, "http", undefined, "file.rest"),
	xml: T("xml", "XML", "xml", undefined, "file.xml"),
	plist: T("plist", L.plist, "xml", undefined, "file.plist"),
	xsd: T("xml", L.xmlSchema, "xml", undefined, "file.xsd"),
	ts: T("ts", "TypeScript", "typescript", undefined, "file.ts"),
	tsx: T("tsx", "TSX", "tsx", undefined, "file.tsx"),
	mts: T("ts", "TypeScript", "typescript", undefined, "file.mts"),
	cts: T("ts", "TypeScript", "typescript", undefined, "file.cts"),
	js: T("js", "JavaScript", "javascript", undefined, "file.js"),
	mjs: T("js", "JavaScript", "javascript", undefined, "file.mjs"),
	cjs: T("js", "JavaScript", "javascript", undefined, "file.cjs"),
	jsx: T("jsx", "JSX", "jsx", undefined, "file.jsx"),
	rs: T("rust", "Rust", "rust", undefined, "file.rs"),
	go: T("go", "Go", "go", undefined, "file.go"),
	py: T("python", "Python", "python", undefined, "file.py"),
	pyi: T("python", L.pythonStub, "python", undefined, "file.pyi"),
	php: T("php", "PHP", "php", undefined, "file.php"),
	html: T("html", "HTML", "html", undefined, "file.html"),
	htm: T("html", "HTML", "html", undefined, "file.htm"),
	css: T("css", "CSS", "css", undefined, "file.css"),
	scss: T("scss", "SCSS", "scss", undefined, "file.scss"),
	sass: T("sass", "Sass", "sass", undefined, "file.sass"),
	less: T("less", "Less", "less", undefined, "file.less"),
	md: T("markdown", "Markdown", "markdown", undefined, "file.md"),
	markdown: T("markdown", "Markdown", "markdown", undefined, "file.md"),
	mdx: T("markdown", "MDX", "markdown", undefined, "file.mdx"),
	txt: T("text", L.text, null, undefined, "file.txt"),
};

const TEXT: FileType = T("text", L.text, null, undefined, "file.txt");

const WORKFLOW = T("workflow", L.githubWorkflow, "yaml", "workflow", "file.yml");
const ACTION = T("workflow", L.githubAction, "yaml", "workflow", "file.yml");

/** Dotfile / Dockerfile-family patterns that need more than a plain lookup. */
function byPattern(name: string, path: string): FileType | undefined {
	if (name.startsWith("dockerfile.") || name.endsWith(".dockerfile") || name.startsWith("containerfile."))
		return BY_NAME.dockerfile;
	if (name.startsWith(".env.") || name.endsWith(".env")) return BY_NAME[".env"];
	if (/^\.[a-z0-9._-]*ignore$/.test(name)) return BY_NAME[".gitignore"];
	if (/^licen[cs]e[-_.]/.test(name) && !/\.(js|ts|json|svg|png|html|css)$/.test(name)) return BY_NAME.license;
	if (/^(docker-)?compose[.-].*\.ya?ml$/.test(name)) return BY_NAME["compose.yaml"];
	if (/^atlas\.[a-z0-9-]+\.ya?ml$/.test(name)) return BY_NAME["atlas.yaml"];
	if (/(^|\/)\.github\/workflows\/[^/]+\.ya?ml$/.test(path)) return WORKFLOW;
	if (/(^|\/)\.github\/(actions\/[^/]+\/)?action\.ya?ml$/.test(path) || /^action\.ya?ml$/.test(name))
		return ACTION;
	if (/^nginx.*\.conf$/.test(name)) return BY_NAME["nginx.conf"];
	if (/^tsconfig\..+\.json$/.test(name)) return BY_NAME["tsconfig.json"];
	if (/^\.(zshrc|bashrc|bash_profile|zprofile|profile|zshenv)$/.test(name))
		return T("shell", name, "shell", undefined, "file.sh");
	return undefined;
}

/** Everything the editor needs to know about a file, from its path alone. */
export function fileType(path: string): FileType {
	const name = (path.split("/").pop() ?? path).toLowerCase();
	const hit = BY_NAME[name] ?? byPattern(name, path);
	if (hit) return hit;
	for (let dot = name.indexOf("."); dot !== -1; dot = name.indexOf(".", dot + 1)) {
		const ext = BY_EXT[name.slice(dot + 1)];
		if (ext) return ext;
	}
	return TEXT;
}

/** Rich views that open as the default (they read better than raw source); every other rich view starts in source. */
export const RICH_DEFAULT_VIEWS: Record<RichView, boolean> = {
	env: true,
	"package-json": true,
	dockerfile: true,
	compose: true,
	makefile: true,
	justfile: true,
	atlas: true,
	manifest: true,
	tsconfig: false,
	workflow: true,
	table: true,
	tree: false,
	ignore: false,
	license: false,
	codeowners: true,
	editorconfig: false,
	procfile: true,
	notebook: true,
};
