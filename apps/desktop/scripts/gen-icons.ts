// Bundles the Material Icon Theme mappings + the SVGs they reference into one
// JSON so the app resolves icons offline. Output is reproducible from
// node_modules, hence gitignored; `dev`/`build`/`check` regenerate it.
import { mkdir, readFile, writeFile, copyFile, readdir } from "node:fs/promises";
import { dirname, join } from "node:path";

const pkg = dirname(Bun.resolveSync("material-icon-theme/package.json", import.meta.dir));
const manifest = JSON.parse(await readFile(join(pkg, "dist/material-icons.json"), "utf8"));
const out = join(import.meta.dir, "../src/lib/icon-data");

type Table = Record<string, string>;
const TABLES = ["fileNames", "fileExtensions", "folderNames", "folderNamesExpanded"] as const;

const used = new Set<string>([manifest.file, manifest.folder, manifest.folderExpanded]);
for (const t of TABLES) for (const id of Object.values(manifest[t] as Table)) used.add(id);

// SVGs are inlined with {@html}; collapse whitespace between tags only.
const svgs: Table = {};
for (const id of [...used].sort()) {
	const def = manifest.iconDefinitions[id];
	if (!def) throw new Error(`material-icon-theme: no iconDefinition for "${id}"`);
	svgs[id] = (await readFile(join(pkg, "dist", def.iconPath), "utf8")).replace(/>\s+</g, "><").trim();
}

await mkdir(out, { recursive: true });
await writeFile(
	join(out, "icons.json"),
	JSON.stringify({
		file: manifest.file,
		folder: manifest.folder,
		folderOpen: manifest.folderExpanded,
		fileNames: manifest.fileNames,
		fileExtensions: manifest.fileExtensions,
		folderNames: manifest.folderNames,
		folderNamesExpanded: manifest.folderNamesExpanded,
		svgs,
	}),
);
await copyFile(join(pkg, "LICENSE"), join(out, "LICENSE"));

// Lucide icon nodes by name, for the project icon picker (search + render by stored name).
// The @lucide/svelte components inline `const iconData = {...}`; read that literal.
const lucideDir = join(
	dirname(Bun.resolveSync("@lucide/svelte/package.json", import.meta.dir)),
	"dist/icons",
);
const lucide: Record<string, unknown> = {};
for (const f of (await readdir(lucideDir)).filter((f) => f.endsWith(".svelte")).sort()) {
	const src = await readFile(join(lucideDir, f), "utf8");
	const m = src.match(/const iconData = (\{.*\});/);
	if (!m) throw new Error(`@lucide/svelte: no iconData in ${f}`);
	const { name, node } = JSON.parse(m[1]);
	lucide[name] = node;
}
await writeFile(join(out, "lucide.json"), JSON.stringify(lucide));
