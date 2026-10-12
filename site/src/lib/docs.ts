import { getCollection, type CollectionEntry } from "astro:content";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const REPO = "https://github.com/codeatlasdev/atlas";

export type DocEntry = CollectionEntry<"docs">;
export interface NavGroup {
	title: string;
	items: { slug: string; title: string }[];
}

/** The page title: `title:` frontmatter, else the first `# Heading` of the markdown. */
export function titleOf(entry: DocEntry): string {
	return entry.data.title ?? entry.body?.match(/^#\s+(.+)$/m)?.[1]?.trim() ?? entry.id;
}

export function editUrl(entry: DocEntry): string {
	return `${REPO}/edit/main/docs/${entry.id}.md`;
}

/** Sidebar groups from docs/_nav.json. Fails the build when the nav and the files disagree. */
export async function loadNav(): Promise<{ groups: NavGroup[]; entries: DocEntry[] }> {
	const entries = await getCollection("docs");
	const byId = new Map(entries.map((e) => [e.id, e]));
	const raw = JSON.parse(readFileSync(resolve(process.cwd(), "../docs/_nav.json"), "utf8")) as {
		title: string;
		items: string[];
	}[];
	const listed = new Set<string>();
	const groups = raw.map((g) => ({
		title: g.title,
		items: g.items.map((slug) => {
			const e = byId.get(slug);
			if (!e) throw new Error(`docs/_nav.json lists "${slug}" but docs/${slug}.md does not exist`);
			listed.add(slug);
			return { slug, title: titleOf(e) };
		}),
	}));
	const orphans = entries.filter((e) => !listed.has(e.id)).map((e) => e.id);
	if (orphans.length) throw new Error(`docs/_nav.json is missing: ${orphans.join(", ")}`);
	return { groups, entries };
}
