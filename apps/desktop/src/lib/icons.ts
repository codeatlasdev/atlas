import data from "./icon-data/icons.json";

/** Mapping tables of material-icon-theme (the subset the resolver reads). */
export interface IconManifest {
	file: string;
	folder: string;
	folderOpen: string;
	fileNames: Record<string, string>;
	fileExtensions: Record<string, string>;
	folderNames: Record<string, string>;
	folderNamesExpanded: Record<string, string>;
}

/**
 * Icon id for a file name: exact name, then the longest dotted suffix
 * (`foo.test.ts` tries `test.ts` before `ts`), then the default.
 */
export function resolveFile(m: IconManifest, name: string): string {
	const n = name.toLowerCase();
	const exact = m.fileNames[n];
	if (exact) return exact;
	for (let dot = n.indexOf("."); dot !== -1; dot = n.indexOf(".", dot + 1)) {
		const hit = m.fileExtensions[n.slice(dot + 1)];
		if (hit) return hit;
	}
	return m.file;
}

/** Icon id for a folder name, by open/closed state. */
export function resolveFolder(m: IconManifest, name: string, open: boolean): string {
	const n = name.toLowerCase();
	return (open ? m.folderNamesExpanded[n] : m.folderNames[n]) ?? (open ? m.folderOpen : m.folder);
}

const manifest: IconManifest & { svgs: Record<string, string> } = data;

/** Inline SVG markup (16x16 viewBox, for `{@html}`). Inline avoids img-src/CSP concerns. */
export const fileIcon = (name: string) => manifest.svgs[resolveFile(manifest, name)];
export const folderIcon = (name: string, open: boolean) => manifest.svgs[resolveFolder(manifest, name, open)];
